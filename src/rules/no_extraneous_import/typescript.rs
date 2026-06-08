//! Detection: at the `program` node, resolve the nearest `package.json`.
//! If there is none, or the file is a test file, stay silent. Otherwise walk
//! every top-level `import_statement`, collapse each bare specifier to its
//! root package name, and flag it if the root appears only in
//! `devDependencies` (not in `dependencies`, `peerDependencies`, or
//! `optionalDependencies`). Relative paths, absolute paths, `node:` imports,
//! and packages absent from every dep section are left to other rules.

use std::path::Path;

use crate::diagnostic::{Diagnostic, Severity};

/// Collapse a bare specifier to the package name that would appear in
/// `package.json`.
/// - `@scope/name/sub` -> `@scope/name`
/// - `pkg/sub`         -> `pkg`
/// - `pkg`             -> `pkg`
fn package_root(specifier: &str) -> &str {
    if specifier.starts_with('@') {
        match specifier.find('/') {
            Some(first_slash) => match specifier[first_slash + 1..].find('/') {
                Some(second_slash) => &specifier[..first_slash + 1 + second_slash],
                None => specifier,
            },
            None => specifier,
        }
    } else {
        match specifier.find('/') {
            Some(slash) => &specifier[..slash],
            None => specifier,
        }
    }
}

fn is_test_file(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    path_str.contains("__tests__")
        || path_str.contains(".test.")
        || path_str.contains(".spec.")
        || path_str.contains(".stories.")
        || path_str.contains(".setup.")
        || path_str.contains("/test/")
        || path_str.contains("/tests/")
        || path_str.contains("/e2e/")
}

fn is_bare_specifier(spec: &str) -> bool {
    !spec.is_empty()
        && !spec.starts_with('.')
        && !spec.starts_with('/')
        && !spec.starts_with("node:")
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    let Some(pkg) = ctx.project.nearest_package_json(ctx.path) else { return; };
    if is_test_file(ctx.path) { return; }
    if crate::rules::path_utils::is_config_file(ctx.path) { return; }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "import_statement" { continue; }
        let Some(source_node) = child.child_by_field_name("source") else { continue; };
        let Ok(raw) = source_node.utf8_text(source) else { continue; };
        let specifier = raw.trim_matches(|c| c == '"' || c == '\'');
        if !is_bare_specifier(specifier) { continue; }

        let root = package_root(specifier);
        let in_runtime = pkg.dependencies.contains_key(root)
            || pkg.peer_dependencies.contains_key(root)
            || pkg.optional_dependencies.contains_key(root);
        if in_runtime { continue; }

        if pkg.dev_dependencies.contains_key(root) {
            let pos = child.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-extraneous-import".into(),
                message: format!(
                    "`{root}` is a devDependency; production code should import from dependencies, peerDependencies, or optionalDependencies."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        // If not in any dep section, stay silent — that's `no-implicit-deps`'s job.
    }
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::diagnostic::Diagnostic;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
        use std::fs;
    use tempfile::TempDir;

    fn setup_with_pkg(pkg_json: &str, file_rel: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), pkg_json).unwrap();
        let file_path = dir.path().join(file_rel);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&file_path, source).unwrap();

        let lang = Language::from_path(&file_path).unwrap();
        let source_file = SourceFile {
            path: file_path.clone(),
            language: lang,
        };
        let refs = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let canon = fs::canonicalize(&file_path).unwrap();

        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &canon, &project, crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn flags_dev_dep_in_prod_code() {
        let pkg = r#"{"dependencies":{"express":"4"},"devDependencies":{"jest":"29"}}"#;
        let d = setup_with_pkg(pkg, "src/server.ts", "import { jest } from 'jest';");
        assert_eq!(d.len(), 1, "got {d:?}");
        assert!(d[0].message.contains("jest"));
    }

    #[test]
    fn allows_dep_in_prod_code() {
        let pkg = r#"{"dependencies":{"express":"4"}}"#;
        let d = setup_with_pkg(pkg, "src/server.ts", "import express from 'express';");
        assert!(d.is_empty(), "got {d:?}");
    }

    #[test]
    fn allows_dev_dep_in_test_file() {
        let pkg = r#"{"devDependencies":{"jest":"29"}}"#;
        let d = setup_with_pkg(
            pkg,
            "src/__tests__/server.test.ts",
            "import { jest } from 'jest';",
        );
        assert!(d.is_empty(), "got {d:?}");
    }

    #[test]
    fn allows_dev_dep_in_spec_file() {
        let pkg = r#"{"devDependencies":{"jest":"29"}}"#;
        let d = setup_with_pkg(pkg, "src/server.spec.ts", "import { jest } from 'jest';");
        assert!(d.is_empty(), "got {d:?}");
    }

    #[test]
    fn allows_dev_dep_in_stories_file() {
        let pkg = r#"{"devDependencies":{"@storybook/react":"7"}}"#;
        let d = setup_with_pkg(
            pkg,
            "src/Button.stories.ts",
            "import { Meta } from '@storybook/react';",
        );
        assert!(d.is_empty(), "got {d:?}");
    }

    #[test]
    fn skips_relative_import() {
        let pkg = r#"{"dependencies":{}}"#;
        let d = setup_with_pkg(pkg, "src/server.ts", "import { foo } from './utils';");
        assert!(d.is_empty(), "got {d:?}");
    }

    #[test]
    fn skips_node_builtin() {
        let pkg = r#"{"dependencies":{}}"#;
        let d = setup_with_pkg(pkg, "src/server.ts", "import fs from 'node:fs';");
        assert!(d.is_empty(), "got {d:?}");
    }

    #[test]
    fn allows_peer_dep() {
        let pkg = r#"{"peerDependencies":{"react":"18"}}"#;
        let d = setup_with_pkg(pkg, "src/app.ts", "import React from 'react';");
        assert!(d.is_empty(), "got {d:?}");
    }

    #[test]
    fn allows_optional_dep() {
        let pkg = r#"{"optionalDependencies":{"fsevents":"2"}}"#;
        let d = setup_with_pkg(pkg, "src/app.ts", "import {} from 'fsevents';");
        assert!(d.is_empty(), "got {d:?}");
    }

    #[test]
    fn handles_scoped_package() {
        let pkg = r#"{"devDependencies":{"@testing-library/react":"14"}}"#;
        let d = setup_with_pkg(
            pkg,
            "src/app.ts",
            "import { render } from '@testing-library/react';",
        );
        assert_eq!(d.len(), 1, "got {d:?}");
    }

    #[test]
    fn handles_scoped_subpath() {
        let pkg = r#"{"devDependencies":{"@testing-library/react":"14"}}"#;
        let d = setup_with_pkg(
            pkg,
            "src/app.ts",
            "import { render } from '@testing-library/react/pure';",
        );
        assert_eq!(d.len(), 1, "got {d:?}");
    }

    #[test]
    fn silent_when_package_absent_from_all_sections() {
        // `no-implicit-deps` handles missing packages; we don't double up.
        let pkg = r#"{"dependencies":{"express":"4"}}"#;
        let d = setup_with_pkg(pkg, "src/server.ts", "import x from 'unlisted-pkg';");
        assert!(d.is_empty(), "got {d:?}");
    }

    #[test]
    fn silent_when_no_package_json() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("src/server.ts");
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        let source = "import { jest } from 'jest';";
        fs::write(&file_path, source).unwrap();
        let source_file = SourceFile {
            path: file_path.clone(),
            language: Language::from_path(&file_path).unwrap(),
        };
        let refs = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let canon = fs::canonicalize(&file_path).unwrap();
        let d = crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &canon, &project, crate::rules::file_ctx::default_static_file_ctx());
        assert!(d.is_empty(), "got {d:?}");
    }

    #[test]
    fn package_root_helper() {
        assert_eq!(package_root("react"), "react");
        assert_eq!(package_root("react-dom/client"), "react-dom");
        assert_eq!(package_root("@scope/pkg"), "@scope/pkg");
        assert_eq!(package_root("@scope/pkg/sub/path"), "@scope/pkg");
    }

    #[test]
    fn prefers_runtime_when_package_in_both_sections() {
        // Duplicated deps are common (peer + dev pairs). If runtime lists it,
        // don't flag — the package is reachable at install time.
        let pkg = r#"{"dependencies":{"react":"18"},"devDependencies":{"react":"18"}}"#;
        let d = setup_with_pkg(pkg, "src/app.ts", "import React from 'react';");
        assert!(d.is_empty(), "got {d:?}");
    }
}
