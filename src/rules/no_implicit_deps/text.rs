//! no-implicit-deps — flag bare imports that are not declared in the nearest
//! `package.json` and are not a Node.js builtin.
//!
//! Resolution steps for each bare import specifier:
//!
//! 1. Relative paths (`./x`, `../y`, `/abs`) — skip.
//! 2. `node:` prefix or known Node builtin root — skip.
//! 3. tsconfig path alias — walk up from the source file looking for the
//!    nearest `tsconfig.json`. Parse `compilerOptions.paths`. If the
//!    specifier's first segment matches any alias key (treating `*` as a
//!    wildcard), skip. `extends` chains are intentionally NOT followed —
//!    covers the common case without the resolution complexity. If a
//!    project uses `extends` and that breaks this rule, the alias key can
//!    still be satisfied by inlining it into the project's own tsconfig.
//! 4. Collapse the specifier to its root package name:
//!    - `@scope/name/sub/path` -> `@scope/name`
//!    - `pkg/sub/path`         -> `pkg`
//!
//!    Then match against the union of `dependencies`, `devDependencies`,
//!    `peerDependencies`, `optionalDependencies`, and the keys of
//!    `engines` (e.g. `engines.vscode` makes `import x from 'vscode'`
//!    valid for VSCode extensions) in the nearest ancestor
//!    `package.json`. Match -> skip.
//! 5. If no `package.json` is found walking up from the source file, the
//!    rule stays silent: we can't prove the dep is missing without a
//!    manifest to compare against, so absence is not an error.
//! 6. Otherwise flag.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug)]
pub struct Check;

const NODE_BUILTINS: &[&str] = &[
    "assert",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "constants",
    "crypto",
    "dgram",
    "dns",
    "domain",
    "events",
    "fs",
    "http",
    "http2",
    "https",
    "module",
    "net",
    "os",
    "path",
    "perf_hooks",
    "process",
    "punycode",
    "querystring",
    "readline",
    "repl",
    "stream",
    "string_decoder",
    "sys",
    "timers",
    "tls",
    "tty",
    "url",
    "util",
    "v8",
    "vm",
    "worker_threads",
    "zlib",
];

fn is_node_builtin(specifier: &str) -> bool {
    if let Some(rest) = specifier.strip_prefix("node:") {
        // node:fs, node:path, etc. — all valid
        return !rest.is_empty();
    }
    // Check root module name (e.g. "fs/promises" -> "fs")
    let root = specifier.split('/').next().unwrap_or(specifier);
    NODE_BUILTINS.contains(&root)
}

/// Extract the module specifier from an import line.
/// Matches: `import ... from 'spec'` / `import ... from "spec"` / `import 'spec'` / `import "spec"`
fn extract_import_specifier(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if !trimmed.starts_with("import ") && !trimmed.starts_with("import\t") {
        return None;
    }
    // Find the last quoted string on the line — that's the specifier
    let spec = extract_quoted(trimmed)?;
    Some(spec)
}

fn extract_quoted(s: &str) -> Option<&str> {
    // Try single quotes first, then double quotes — pick the last occurrence
    let single = s.rfind('\'').and_then(|end| {
        let before = &s[..end];
        let start = before.rfind('\'')?;
        Some(&s[start + 1..end])
    });
    let double = s.rfind('"').and_then(|end| {
        let before = &s[..end];
        let start = before.rfind('"')?;
        Some(&s[start + 1..end])
    });
    // Return whichever appears later in the string (the from-specifier, not a type string)
    match (single, double) {
        (Some(a), Some(b)) => {
            let a_pos = s.rfind(&format!("'{a}'")).unwrap_or(0);
            let b_pos = s.rfind(&format!("\"{b}\"")).unwrap_or(0);
            if a_pos > b_pos {
                Some(a)
            } else {
                Some(b)
            }
        }
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn is_bare_specifier(spec: &str) -> bool {
    !spec.starts_with('.') && !spec.starts_with('/')
}

/// Collapse a bare specifier to the name that would appear in `package.json`.
/// - `@scope/name/sub` -> `@scope/name`
/// - `pkg/sub`         -> `pkg`
/// - `pkg`             -> `pkg`
fn root_package_name(spec: &str) -> &str {
    if let Some(rest) = spec.strip_prefix('@') {
        // Scoped: keep first TWO segments.
        let mut parts = rest.splitn(3, '/');
        match (parts.next(), parts.next()) {
            (Some(scope), Some(name)) => {
                // Length = "@" + scope + "/" + name
                let len = 1 + scope.len() + 1 + name.len();
                &spec[..len]
            }
            _ => spec, // Malformed scoped name — return as-is; lookup will just fail.
        }
    } else {
        spec.split('/').next().unwrap_or(spec)
    }
}

/// Walk up from `file` looking for the given manifest filename.
fn find_manifest(file: &Path, name: &str) -> Option<std::path::PathBuf> {
    // We can't rely on canonicalize here: unit tests use virtual paths like "t.ts".
    // The real engine always hands us absolute paths, but tests may not — so
    // just walk parents and accept None when we fall off the root.
    let mut current = file.parent();
    while let Some(dir) = current {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        current = dir.parent();
    }
    None
}

/// Load the union of all dep sections from `package.json`. Returns `None`
/// when the file can't be read or parsed — treated as "no manifest found".
fn load_package_deps(manifest: &Path) -> Option<HashSet<String>> {
    let raw = std::fs::read_to_string(manifest).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let mut deps = HashSet::new();
    for section in [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
        // `engines` keys name host runtimes whose APIs are importable as
        // bare specifiers without appearing in the dep sections — e.g.
        // VSCode extensions declare `engines.vscode` and then
        // `import vscode from 'vscode'`. Treat keys as valid package names.
        "engines",
    ] {
        if let Some(obj) = value.get(section).and_then(|v| v.as_object()) {
            for key in obj.keys() {
                deps.insert(key.clone());
            }
        }
    }
    Some(deps)
}

/// Parse `compilerOptions.paths` keys from a tsconfig.json. Returns the
/// list of alias prefixes (with any trailing `/*` stripped). An empty
/// vector is also used to represent "no paths block" or "parse failure"
/// — either way we just don't alias-match.
fn load_tsconfig_alias_prefixes(manifest: &Path) -> Vec<String> {
    let Ok(raw) = std::fs::read_to_string(manifest) else {
        return Vec::new();
    };
    // tsconfig.json frequently has `//` comments and trailing commas.
    // serde_json rejects both. Strip line comments first; trailing commas
    // are rarer and we accept a silent parse failure there rather than
    // pulling in a full JSONC parser for v1.
    let stripped = strip_line_comments(&raw);
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&stripped) else {
        return Vec::new();
    };
    let Some(paths) = value
        .get("compilerOptions")
        .and_then(|v| v.get("paths"))
        .and_then(|v| v.as_object())
    else {
        return Vec::new();
    };
    paths
        .keys()
        .map(|k| k.strip_suffix("/*").unwrap_or(k.as_str()).to_string())
        .collect()
}

/// Strip `//`-to-end-of-line comments that tsconfig often contains. Leaves
/// `//` inside string literals alone.
fn strip_line_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for line in s.lines() {
        let mut in_string = false;
        let mut escape = false;
        let mut comment_start: Option<usize> = None;
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let c = bytes[i] as char;
            if escape {
                escape = false;
            } else if c == '\\' && in_string {
                escape = true;
            } else if c == '"' {
                in_string = !in_string;
            } else if !in_string && c == '/' && i + 1 < bytes.len() && bytes[i + 1] as char == '/' {
                comment_start = Some(i);
                break;
            }
            i += 1;
        }
        let keep = comment_start.map_or(line, |c| &line[..c]);
        out.push_str(keep);
        out.push('\n');
    }
    out
}

/// True if `spec` matches any alias prefix (exact or `prefix/...`).
fn matches_alias(spec: &str, alias_prefixes: &[String]) -> bool {
    alias_prefixes.iter().any(|p| {
        if p.is_empty() {
            return false;
        }
        if spec == p.as_str() {
            return true;
        }
        if let Some(rest) = spec.strip_prefix(p.as_str()) {
            return rest.starts_with('/');
        }
        false
    })
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // Find the nearest manifests. If there's no package.json anywhere
        // above this file we can't prove anything; stay silent.
        let Some(pkg_manifest) = find_manifest(ctx.path, "package.json") else {
            return Vec::new();
        };
        let Some(deps) = load_package_deps(&pkg_manifest) else {
            return Vec::new();
        };
        let alias_prefixes = find_manifest(ctx.path, "tsconfig.json")
            .map(|p| load_tsconfig_alias_prefixes(&p))
            .unwrap_or_default();

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let Some(spec) = extract_import_specifier(line) else {
                continue;
            };
            if !is_bare_specifier(spec) {
                continue;
            }
            if is_node_builtin(spec) {
                continue;
            }
            if matches_alias(spec, &alias_prefixes) {
                continue;
            }
            let root = root_package_name(spec);
            if deps.contains(root) {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "no-implicit-deps".into(),
                message: format!(
                    "Bare import `{spec}` is not listed in package.json (checked root `{root}`)."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Build a temp project with an optional package.json body and an
    /// optional tsconfig body, then run the check on a source file placed
    /// inside `src/`.
    fn run_in_project(
        package_json: Option<&str>,
        tsconfig: Option<&str>,
        source: &str,
    ) -> (TempDir, Vec<Diagnostic>) {
        let dir = TempDir::new().unwrap();
        if let Some(body) = package_json {
            fs::write(dir.path().join("package.json"), body).unwrap();
        }
        if let Some(body) = tsconfig {
            fs::write(dir.path().join("tsconfig.json"), body).unwrap();
        }
        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();
        let file = src_dir.join("t.ts");
        fs::write(&file, source).unwrap();
        let diags = Check.check(&CheckCtx::for_test(&file, source));
        (dir, diags)
    }

    #[test]
    fn flags_bare_specifier_when_absent() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "dependencies": {} }"#),
            None,
            "import { foo } from 'lodash';",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_scoped_package_when_absent() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "dependencies": {} }"#),
            None,
            "import { bar } from '@acme/utils';",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_relative_import() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "dependencies": {} }"#),
            None,
            "import { foo } from './utils';",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_node_builtin() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "dependencies": {} }"#),
            None,
            "import fs from 'fs';",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_node_prefixed() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "dependencies": {} }"#),
            None,
            "import { readFile } from 'node:fs';",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_node_builtin_subpath() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "dependencies": {} }"#),
            None,
            "import { readFile } from 'fs/promises';",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_declared_dependency() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "dependencies": { "react": "^19.0.0" } }"#),
            None,
            "import React from 'react';",
        );
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn ignores_scoped_package_subpath() {
        // The root "@scope/pkg" is declared; a subpath import should match.
        let (_d, diags) = run_in_project(
            Some(r#"{ "dependencies": { "@scope/pkg": "^1.0.0" } }"#),
            None,
            "import x from '@scope/pkg/sub/path';",
        );
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn ignores_tsconfig_path_alias() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "dependencies": {} }"#),
            Some(r#"{ "compilerOptions": { "paths": { "~/*": ["./src/*"] } } }"#),
            "import x from '~/components/ui/alert';",
        );
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn ignores_dev_dependency() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "devDependencies": { "vitest": "^1.0.0" } }"#),
            None,
            "import { test } from 'vitest';",
        );
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn ignores_peer_dependency() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "peerDependencies": { "react": "^19.0.0" } }"#),
            None,
            "import React from 'react';",
        );
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn ignores_optional_dependency() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "optionalDependencies": { "fsevents": "^2.0.0" } }"#),
            None,
            "import {} from 'fsevents';",
        );
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn flags_truly_missing() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "dependencies": { "react": "^19.0.0" } }"#),
            None,
            "import x from 'not-a-real-pkg';",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn silent_when_no_package_json() {
        // No package.json anywhere above — rule cannot prove anything.
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("t.ts");
        let src = "import x from 'lodash';";
        fs::write(&file, src).unwrap();
        let diags = Check.check(&CheckCtx::for_test(&file, src));
        assert!(diags.is_empty(), "expected silence, got {diags:?}");
    }

    #[test]
    fn tsconfig_with_line_comments_still_parses() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "dependencies": {} }"#),
            Some(
                "{\n  // tsconfig comment\n  \"compilerOptions\": { \"paths\": { \"~/*\": [\"./src/*\"] } }\n}",
            ),
            "import x from '~/lib/utils';",
        );
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn root_package_name_collapses_scoped_subpath() {
        assert_eq!(root_package_name("@base-ui/react/button"), "@base-ui/react");
        assert_eq!(root_package_name("@scope/pkg"), "@scope/pkg");
        assert_eq!(root_package_name("react-dom/client"), "react-dom");
        assert_eq!(root_package_name("react"), "react");
    }

    #[test]
    fn allows_engines_vscode() {
        // VSCode extensions: engines.vscode implies `import vscode from 'vscode'`.
        let (_d, diags) = run_in_project(
            Some(r#"{ "engines": { "vscode": "^1.85.0" } }"#),
            None,
            "import vscode from 'vscode';",
        );
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn allows_engines_electron() {
        // Same pattern with a different host runtime.
        let (_d, diags) = run_in_project(
            Some(r#"{ "engines": { "electron": "^28.0.0" } }"#),
            None,
            "import { app } from 'electron';",
        );
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn still_flags_unlisted_bare_import_with_only_engines() {
        // engines.vscode does NOT whitelist arbitrary imports — only 'vscode' itself.
        let (_d, diags) = run_in_project(
            Some(r#"{ "engines": { "vscode": "^1.85.0" } }"#),
            None,
            "import x from 'foo';",
        );
        assert_eq!(diags.len(), 1);
    }
}
