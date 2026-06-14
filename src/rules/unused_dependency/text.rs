//! unused-dependency detection — diff `package.json#dependencies` against the
//! set of bare specifiers the import index actually saw.
//!
//! Per-project guard: the check is project-wide, but the rule engine still
//! invokes us once per file. To emit each diagnostic exactly once we run only
//! when the current path is the lexicographically smallest indexed path —
//! a deterministic anchor that doesn't depend on filesystem iteration order.
//!
//! Each diagnostic is anchored on `package.json` itself: an unused dependency
//! has no importer to point at, and the remediation is editing `package.json`.
//!
//! Skips:
//!   - `@types/*` packages — type definitions are consumed by the type
//!     checker, not by `import` statements.
//!   - Tooling packages used without explicit imports (`typescript`, `eslint`,
//!     `prettier`, `webpack`, `vite`, `turbo`, `jest`, `vitest`, `mocha`,
//!     `cypress`, `playwright`).
//!   - CLI-runner packages whose binary is invoked by a `scripts` command
//!     (`@changesets/cli` → `changeset publish`) — they run as a binary, not
//!     as an `import`.
//!   - Packages named by a string literal in a build/tooling config file
//!     (Babel presets/plugins, Jest `preset`/`transform`, ESLint `extends`/
//!     `plugins`, PostCSS, …). These tools resolve their dependencies from
//!     config strings, not `import` statements, so the import index never
//!     sees them.
//!
//! Only inspects `dependencies` (production). `devDependencies`,
//! `peerDependencies`, and `optionalDependencies` are out of scope — they
//! have legitimate non-import use cases (CI scripts, type-only consumers,
//! optional native bindings).

use rustc_hash::FxHashSet;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::path_utils::is_config_file;

const RULE_ID: &str = "unused-dependency";

/// Packages used by config files / CLI tooling rather than by `import`
/// statements. Adding to this list is conservative — false-positive flags
/// here are noisy, false-negatives just miss a few unused deps.
const TOOLING_ALLOWLIST: &[&str] = &[
    "typescript",
    "eslint",
    "prettier",
    "webpack",
    "vite",
    "turbo",
    "jest",
    "vitest",
    "mocha",
    "cypress",
    "playwright",
    // Babel engine: loaded implicitly by the toolchain to run a babel config,
    // never `import`-ed by source. Its presets/plugins are caught separately by
    // the config-string scan; the engine itself never appears as a config string.
    "@babel/core",
];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let Some(pkg) = ctx.project.package_json.as_deref() else {
            return Vec::new();
        };

        let index = ctx.project.import_index();

        let canon = index.canonical(ctx.path);
        let Some(anchor) = ctx.project.anchor_path() else {
            return Vec::new();
        };
        if anchor != canon {
            return Vec::new();
        }

        let bare = index.bare_specifiers();
        let manifest_path = ctx
            .project
            .project_root
            .as_ref()
            .map(|r| r.join("package.json"))
            .unwrap_or_else(|| ctx.path.to_path_buf());
        let mut diagnostics = Vec::new();
        let extra_tooling: FxHashSet<&str> = ctx.project.framework_tooling_deps().collect();
        let config_refs = config_string_refs(index.indexed_paths());
        for dep in pkg.dependencies.keys() {
            if is_skipped(dep, &extra_tooling) {
                continue;
            }
            if bare.contains_key(dep) {
                continue;
            }
            // A CLI-runner package (`@changesets/cli`) is run via a `scripts`
            // command, never ES-imported, so the import index sees no usage.
            if pkg.scripts_invoke_dep_binary(dep) {
                continue;
            }
            // Build tools resolve dependencies from string literals in their
            // config files (Babel presets/plugins, Jest `preset`/`transform`,
            // ESLint `extends`/`plugins`, …), so the import index never records
            // them. A config string naming the package counts as usage.
            if config_refs.contains(dep) {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: manifest_path.clone().into(),
                line: 1,
                column: 1,
                rule_id: RULE_ID.into(),
                message: format!(
                    "dependency `{dep}` is declared in package.json but never imported. \
                     Remove it, or add an import if it's actually needed."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

fn is_skipped(dep: &str, extra_tooling: &FxHashSet<&str>) -> bool {
    if dep.starts_with("@types/") {
        return true;
    }
    if TOOLING_ALLOWLIST.contains(&dep) {
        return true;
    }
    extra_tooling.contains(dep)
}

/// Package names referenced by a string literal anywhere in the project's
/// config files. Build/test tooling (Babel, Jest, ESLint, PostCSS, …) names
/// its dependencies as config strings (`presets: ['@babel/preset-env']`),
/// never as `import` statements, so those packages are absent from the import
/// index. Each string is normalized to its package-name form
/// (`@scope/pkg/sub` → `@scope/pkg`, `lodash/fp` → `lodash`) so subpath and
/// preset references match the declared dependency. Scoped to config files and
/// to package-name-shaped strings so a genuinely unused dependency — named in
/// no config and imported nowhere — is still flagged.
fn config_string_refs<'a>(indexed: impl Iterator<Item = &'a std::path::Path>) -> FxHashSet<String> {
    let mut refs = FxHashSet::default();
    for path in indexed.filter(|p| is_config_file(p)) {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        for literal in string_literals(&source) {
            if let Some(name) = package_name_of(literal) {
                refs.insert(name);
            }
        }
    }
    refs
}

/// Contents of every single-, double-, and backtick-quoted string literal in
/// `source`, without interpreting escapes (escaped quotes simply end the
/// literal — good enough for config files, which name packages with plain
/// strings). Yields the raw inner text; callers decide what shape to keep.
fn string_literals(source: &str) -> impl Iterator<Item = &str> {
    let bytes = source.as_bytes();
    let mut start = 0;
    std::iter::from_fn(move || {
        while start < bytes.len() {
            let quote = bytes[start];
            if quote == b'\'' || quote == b'"' || quote == b'`' {
                let inner_start = start + 1;
                if let Some(offset) = bytes[inner_start..].iter().position(|&b| b == quote) {
                    let inner_end = inner_start + offset;
                    let literal = &source[inner_start..inner_end];
                    start = inner_end + 1;
                    return Some(literal);
                }
                // Unterminated quote: stop scanning this source.
                start = bytes.len();
                return None;
            }
            start += 1;
        }
        None
    })
}

/// The npm package-name form of a config string, or `None` when the string is
/// not shaped like a package reference. `@scope/pkg/sub` → `@scope/pkg`,
/// `lodash/fp` → `lodash`. Rejects relative paths, absolute paths, and URLs so
/// a `'./setup.ts'` config entry can never alias a declared dependency.
fn package_name_of(literal: &str) -> Option<String> {
    if literal.is_empty()
        || literal.starts_with('.')
        || literal.starts_with('/')
        || literal.contains(':')
    {
        return None;
    }
    if let Some(scoped) = literal.strip_prefix('@') {
        let (scope, rest) = scoped.split_once('/')?;
        if scope.is_empty() {
            return None;
        }
        let name = rest.split('/').next().unwrap_or("");
        if name.is_empty() {
            return None;
        }
        return Some(format!("@{scope}/{name}"));
    }
    Some(literal.split('/').next().unwrap_or("").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::file_ctx::FileCtx;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn run_on_project(
        files: &[(&str, &str)],
        package_json: &str,
        target_rel: &str,
    ) -> (TempDir, Vec<Diagnostic>) {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), package_json).unwrap();

        let mut source_files: Vec<SourceFile> = Vec::new();
        for (rel, content) in files {
            let p = dir.path().join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&p, content).unwrap();
            let lang = Language::from_path(&p).unwrap();
            source_files.push(SourceFile {
                path: p,
                language: lang,
            });
        }
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        let target_path: PathBuf = dir.path().join(target_rel);
        let source = fs::read_to_string(&target_path).unwrap();
        let file_ctx = FileCtx::build(&target_path, &source, Language::TypeScript, &project);
        let ctx = CheckCtx {
            path: &target_path,
            path_arc: Arc::from(target_path.as_path()),
            source: &source,
            config: &config,
            project: &project,
            file: &file_ctx, lang: crate::files::Language::TypeScript,
        };
        let diags = Check.check(&ctx);
        (dir, diags)
    }

    #[test]
    fn flags_unused_dep() {
        let pkg = r#"{
            "name": "demo",
            "dependencies": { "lodash": "^4.0.0" }
        }"#;
        let files: Vec<(&str, &str)> = vec![("a.ts", "export const x = 1;")];
        let (_dir, diags) = run_on_project(&files, pkg, "a.ts");
        assert_eq!(diags.len(), 1, "lodash is declared but never imported");
        assert_eq!(diags[0].rule_id, "unused-dependency");
        assert!(
            diags[0].message.contains("lodash"),
            "message should name the unused dep, got: {}",
            diags[0].message
        );
    }

    #[test]
    fn allows_used_dep() {
        let pkg = r#"{
            "name": "demo",
            "dependencies": { "lodash": "^4.0.0" }
        }"#;
        let files: Vec<(&str, &str)> =
            vec![("a.ts", "import _ from 'lodash';\nexport const x = _;")];
        let (_dir, diags) = run_on_project(&files, pkg, "a.ts");
        assert!(
            diags.is_empty(),
            "lodash is imported, no diagnostic expected, got: {diags:?}"
        );
    }

    #[test]
    fn skips_at_types_packages() {
        let pkg = r#"{
            "name": "demo",
            "dependencies": { "@types/node": "^20.0.0" }
        }"#;
        let files: Vec<(&str, &str)> = vec![("a.ts", "export const x = 1;")];
        let (_dir, diags) = run_on_project(&files, pkg, "a.ts");
        assert!(
            diags.is_empty(),
            "@types/* packages must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn repo_scope_dep_used_in_sibling_file_not_flagged() {
        // The issue's systematic FP: linting a backend file that doesn't
        // import a dep must not flag that dep when *another* file imports it.
        // A dependency is unused only when *no* file in the repo imports it.
        let pkg = r#"{
            "name": "amadeo",
            "dependencies": {
                "lodash": "^4.0.0",
                "@better-auth/i18n": "^1.0.0"
            }
        }"#;
        let files: Vec<(&str, &str)> = vec![
            ("api/server.ts", "export const handler = () => 1;"),
            ("ui/app.ts", "import _ from 'lodash';\nexport const x = _;"),
        ];
        let (_dir, diags) = run_on_project(&files, pkg, "api/server.ts");
        assert_eq!(
            diags.len(),
            1,
            "lodash is imported in ui/app.ts; only @better-auth/i18n is unused repo-wide: {diags:?}"
        );
        assert!(
            diags[0].message.contains("@better-auth/i18n"),
            "message should name the scoped unused dep, got: {}",
            diags[0].message
        );
        assert!(
            diags[0].path.ends_with("package.json"),
            "diagnostic must anchor on package.json, not a source file, got: {:?}",
            diags[0].path
        );
    }

    #[test]
    fn skips_tooling_packages() {
        let pkg = r#"{
            "name": "demo",
            "dependencies": {
                "typescript": "^5.0.0",
                "jest": "^29.0.0",
                "vite": "^5.0.0"
            }
        }"#;
        let files: Vec<(&str, &str)> = vec![("a.ts", "export const x = 1;")];
        let (_dir, diags) = run_on_project(&files, pkg, "a.ts");
        assert!(
            diags.is_empty(),
            "tooling packages must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn skips_cli_runner_invoked_via_scripts() {
        // Issue #2076 pattern 1: `@changesets/cli` and `@manypkg/cli` provide
        // binaries (`changeset`, `manypkg`) run by package.json scripts, never
        // ES-imported. Their bin name appears as a command head in `scripts`.
        let pkg = r#"{
            "name": "demo",
            "scripts": {
                "release": "changeset publish",
                "check": "manypkg check"
            },
            "dependencies": {
                "@changesets/cli": "^2.0.0",
                "@manypkg/cli": "^0.21.0"
            }
        }"#;
        let files: Vec<(&str, &str)> = vec![("a.ts", "export const x = 1;")];
        let (_dir, diags) = run_on_project(&files, pkg, "a.ts");
        assert!(
            diags.is_empty(),
            "CLI runner packages invoked via scripts must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn allows_dep_imported_only_in_storybook_config_dir() {
        // Issue #1769: `@storybook/manager-api` and `@storybook/theming` are
        // imported only from `.storybook/`. Driving the real directory walker
        // (not the explicit-file harness) proves those imports are discovered
        // and the packages are not flagged unused.
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::write(
            root.join("package.json"),
            r#"{
                "name": "demo",
                "dependencies": {
                    "@storybook/manager-api": "^8.6.12",
                    "@storybook/theming": "^8.6.12"
                }
            }"#,
        )
        .unwrap();
        fs::create_dir(root.join(".storybook")).unwrap();
        fs::write(
            root.join(".storybook/manager.ts"),
            "import { addons } from \"@storybook/manager-api\";\naddons.setConfig({ panelPosition: \"bottom\" });",
        )
        .unwrap();
        fs::write(
            root.join(".storybook/preview.ts"),
            "import { themes } from \"@storybook/theming\";\nexport const x = themes;",
        )
        .unwrap();
        fs::write(root.join("a.ts"), "export const x = 1;").unwrap();

        let source_files =
            crate::files::discover(&crate::cli::ScanMode::All(root.to_path_buf())).unwrap();
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        // Run the check on the rule's own anchor (the lexicographically smallest
        // indexed path) so the per-project diagnostics actually fire.
        let anchor = project.anchor_path().expect("anchor path");
        let source = fs::read_to_string(&anchor).unwrap();
        let file_ctx = FileCtx::build(&anchor, &source, Language::TypeScript, &project);
        let ctx = CheckCtx {
            path: &anchor,
            path_arc: Arc::from(anchor.as_path()),
            source: &source,
            config: &config,
            project: &project,
            file: &file_ctx,
            lang: Language::TypeScript,
        };
        let diags = Check.check(&ctx);

        assert!(
            diags.is_empty(),
            "packages imported in .storybook/ must not be flagged unused, got: {diags:?}"
        );
    }

    #[test]
    fn skips_babel_preset_referenced_only_as_config_string() {
        // Issue #1607: Babel resolves presets/plugins from string literals in
        // `babel.config.js`, never as `import` statements, so the import index
        // never records them. The config string must count as usage.
        let pkg = r#"{
            "name": "demo",
            "dependencies": {
                "@babel/core": "^7.23.3",
                "@babel/preset-env": "^7.23.3",
                "@babel/preset-react": "^7.23.3",
                "@babel/preset-typescript": "^7.23.3",
                "@babel/proposal-class-properties": "^7.18.6"
            }
        }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "babel.config.js",
                "module.exports = {\n  presets: [\n    ['@babel/preset-env', { targets: { node: 18 } }],\n    ['@babel/preset-react', { runtime: 'automatic' }],\n    ['@babel/preset-typescript', { isTSX: true }],\n  ],\n  plugins: ['@babel/proposal-class-properties'],\n};\n",
            ),
            ("a.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, pkg, "a.ts");
        assert!(
            diags.is_empty(),
            "Babel engine + presets/plugins named as config strings must not be flagged unused, got: {diags:?}"
        );
    }

    #[test]
    fn skips_dep_referenced_via_subpath_config_string() {
        // A config string can reference a package by subpath
        // (`jest-preset-foo/jsx`); the package-name form must still match the
        // declared dependency.
        let pkg = r#"{
            "name": "demo",
            "dependencies": { "@my/preset": "^1.0.0" }
        }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "jest.config.js",
                "module.exports = { preset: '@my/preset/jsx', setupFiles: ['@my/preset/setup'] };\n",
            ),
            ("a.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, pkg, "a.ts");
        assert!(
            diags.is_empty(),
            "a dependency referenced by subpath in a config file must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn flags_dep_referenced_in_no_config_and_imported_nowhere() {
        // Negative space: a dependency that is neither imported nor named by any
        // config string is genuinely unused and must still be flagged. The
        // config-string carve-out must not mask it just because a config file
        // exists in the project.
        let pkg = r#"{
            "name": "demo",
            "dependencies": {
                "@babel/preset-env": "^7.23.3",
                "left-pad": "^1.3.0"
            }
        }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "babel.config.js",
                "module.exports = { presets: ['@babel/preset-env'] };\n",
            ),
            ("a.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, pkg, "a.ts");
        assert_eq!(
            diags.len(),
            1,
            "only left-pad is unused; @babel/preset-env is named in the config: {diags:?}"
        );
        assert!(
            diags[0].message.contains("left-pad"),
            "message should name the genuinely unused dep, got: {}",
            diags[0].message
        );
    }

    #[test]
    fn config_string_does_not_match_via_relative_path_or_substring() {
        // A relative-path config string (`./babel-helper`) must not alias a
        // declared dependency named `babel-helper`, and a substring of a string
        // (`react` inside `'react-router'`) must not mark `react` used.
        let pkg = r#"{
            "name": "demo",
            "dependencies": {
                "babel-helper": "^1.0.0",
                "react": "^18.0.0"
            }
        }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "babel.config.js",
                "module.exports = { plugins: ['./babel-helper', 'react-router'] };\n",
            ),
            ("a.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, pkg, "a.ts");
        assert_eq!(
            diags.len(),
            2,
            "neither babel-helper (relative path) nor react (substring) is referenced: {diags:?}"
        );
    }

    #[test]
    fn skips_eslint_flat_config_plugin() {
        // Issue #2076 pattern 2: an ESLint flat config imports a plugin that no
        // app source file imports. The import inside `eslint.config.js` is the
        // package's only usage and must count as evidence.
        let pkg = r#"{
            "name": "demo",
            "dependencies": {
                "eslint-plugin-import-x": "^4.0.0"
            }
        }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "eslint.config.js",
                "import importPlugin from 'eslint-plugin-import-x';\nexport default [importPlugin.configs.recommended];",
            ),
            ("a.ts", "export const x = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, pkg, "a.ts");
        assert!(
            diags.is_empty(),
            "ESLint flat config plugins imported in eslint.config.js must not be flagged, got: {diags:?}"
        );
    }
}
