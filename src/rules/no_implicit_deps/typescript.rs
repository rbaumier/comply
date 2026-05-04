//! no-implicit-deps backend — flag bare `import` specifiers that are not
//! declared in the nearest ancestor `package.json` and are not Node.js
//! builtins.
//!
//! Resolution steps for each bare import specifier:
//!
//! 1. Relative paths (`./x`, `../y`, `/abs`) — skip.
//! 2. `node:` prefix or known Node builtin root — skip.
//! 3. tsconfig path alias — walk up from the source file looking for the
//!    nearest `tsconfig.json`. If the specifier's first segment matches
//!    any alias key (treating `*` as a wildcard), skip. `extends` chains
//!    are intentionally NOT followed — covers the common case without
//!    the resolution complexity.
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

use super::{
    is_bare_specifier, is_node_builtin, is_virtual_module, matches_alias, root_package_name,
    strip_quotes,
};

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    // Stay silent if there's no `package.json` anywhere above this file —
    // we can't prove a dep is missing without a manifest to compare
    // against. The lookup is cached on `ProjectCtx` and monorepo-safe
    // (picks the workspace manifest, not the root).
    let Some(pkg) = ctx.project.nearest_package_json(ctx.path) else {
        return;
    };
    let alias_prefixes = ctx
        .project
        .nearest_tsconfig(ctx.path)
        .map(|t| t.alias_prefixes())
        .unwrap_or_default();

    let Some(src_node) = node.child_by_field_name("source") else { return; };
    let Ok(raw) = src_node.utf8_text(source) else { return; };
    let spec = strip_quotes(raw);

    if !is_bare_specifier(spec) {
        return;
    }
    if is_node_builtin(spec) {
        return;
    }
    if is_virtual_module(spec) {
        return;
    }
    if matches_alias(spec, &alias_prefixes) {
        return;
    }
    let root = root_package_name(spec);
    if pkg.has_dep_or_engine(root) {
        return;
    }
    // Workspace fallback: check workspace package names and the root
    // package.json when the nearest manifest doesn't list the dep.
    if ctx.project.workspace_package_names().iter().any(|n| n == root) {
        return;
    }
    if let Some(root_pkg) = &ctx.project.package_json {
        if root_pkg.has_dep_or_engine(root) {
            return;
        }
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-implicit-deps".into(),
        message: format!(
            "Bare import `{spec}` is not listed in package.json (checked root `{root}`)."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::fs;
    use tempfile::TempDir;

    fn parse(source: &str) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("grammar should load");
        parser
            .parse(source, None)
            .expect("parser should produce a tree")
    }

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
        let tree = parse(source);
        let diags = Check.check(&CheckCtx::for_test(&file, source), &tree);
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
    fn ignores_at_slash_path_alias() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "dependencies": {} }"#),
            Some(r#"{ "compilerOptions": { "paths": { "@/*": ["./src/*"] } } }"#),
            "import { http } from '@/http';",
        );
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn ignores_k6_builtin() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "dependencies": {} }"#),
            None,
            "import http from 'k6/http';",
        );
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn ignores_url_import() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "dependencies": {} }"#),
            None,
            "import { check } from 'https://jslib.k6.io/k6-utils/1.4.0/index.js';",
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
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("t.ts");
        let src = "import x from 'lodash';";
        fs::write(&file, src).unwrap();
        let tree = parse(src);
        let diags = Check.check(&CheckCtx::for_test(&file, src), &tree);
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
        let (_d, diags) = run_in_project(
            Some(r#"{ "engines": { "vscode": "^1.85.0" } }"#),
            None,
            "import vscode from 'vscode';",
        );
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn allows_engines_electron() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "engines": { "electron": "^28.0.0" } }"#),
            None,
            "import { app } from 'electron';",
        );
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn ignores_virtual_module_prefixes() {
        for spec in &[
            "virtual:my-plugin",
            "@theme/Layout",
            "@docusaurus/preset-classic",
            "@internal/shared",
        ] {
            let src = format!("import x from '{spec}';");
            let (_d, diags) = run_in_project(
                Some(r#"{ "dependencies": {} }"#),
                None,
                &src,
            );
            assert!(diags.is_empty(), "virtual module `{spec}` should be skipped, got {diags:?}");
        }
    }

    #[test]
    fn still_flags_unlisted_bare_import_with_only_engines() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "engines": { "vscode": "^1.85.0" } }"#),
            None,
            "import x from 'foo';",
        );
        assert_eq!(diags.len(), 1);
    }
}
