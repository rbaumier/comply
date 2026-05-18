//! avoid-importing-barrel-files backend — flag relative imports that
//! resolve to a barrel file.
//!
//! Barrel heuristic on the import path alone:
//! - ends with `/index` or `/index.{ts,tsx,js,jsx,mjs,cjs}` — explicit barrel,
//! - ends with a trailing slash (`./foo/`) — directory import that Node
//!   resolves to `index.*`,
//! - is a bare `.` or `..` — the current/parent directory barrel.
//!
//! Only relative imports (`.`/`..`) are checked. Package imports (`react`,
//! `@scope/pkg`) are left alone — tree-shakers handle those, and flagging
//! them would be far too noisy.
//!
//! Skipped when the importing file lives under a `routes/` directory: that's
//! the TanStack Router file-system convention where `index.tsx` is the leaf
//! route module for a segment, not a re-export hub.

use crate::diagnostic::{Diagnostic, Severity};
use std::path::Path;

const INDEX_SUFFIXES: &[&str] = &[
    "/index",
    "/index.ts",
    "/index.tsx",
    "/index.js",
    "/index.jsx",
    "/index.mjs",
    "/index.cjs",
];

fn strip_quotes(s: &str) -> &str {
    s.trim_matches(|c| c == '\'' || c == '"' || c == '`')
}

fn is_barrel_path(module: &str) -> bool {
    if !module.starts_with('.') {
        return false;
    }
    if module == "." || module == ".." {
        return true;
    }
    if module.ends_with('/') {
        return true;
    }
    INDEX_SUFFIXES.iter().any(|s| module.ends_with(s))
}

fn is_tanstack_route_file(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str() == "routes")
}

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    let Some(src_node) = node.child_by_field_name("source") else { return };
    let raw = src_node.utf8_text(source).unwrap_or("");
    let module = strip_quotes(raw);
    if !is_barrel_path(module) {
        return;
    }
    if is_tanstack_route_file(ctx.path) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "avoid-importing-barrel-files".into(),
        message: format!(
            "Import from barrel file `{module}` — import directly from the source module instead."
        ),
        severity: Severity::Warning,
        span: Some((node.start_byte(), node.end_byte() - node.start_byte())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_explicit_index_import() {
        let d = run_on("import { foo } from './utils/index';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("barrel file"));
    }

    #[test]
    fn flags_explicit_index_with_extension() {
        let d = run_on("import { foo } from './utils/index.ts';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_directory_with_trailing_slash() {
        let d = run_on("import { foo } from './utils/';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_current_dir_import() {
        let d = run_on("import { foo } from '.';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_parent_dir_import() {
        let d = run_on("import { foo } from '..';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_direct_file_import() {
        assert!(run_on("import { foo } from './utils/string';").is_empty());
    }

    #[test]
    fn allows_package_import() {
        // Bare package names are NOT checked — tree-shakers handle npm packages.
        assert!(run_on("import { useState } from 'react';").is_empty());
    }

    #[test]
    fn allows_file_named_index_like() {
        // `./indexer` is not a barrel; only `/index` or `/index.*`.
        assert!(run_on("import { foo } from './indexer';").is_empty());
    }

    #[test]
    fn allows_index_import_from_tanstack_route_file() {
        // Regression for #160: TanStack route files (under `routes/`) commonly
        // import `./<segment>/index` as a leaf route module, not a barrel.
        let d = crate::rules::test_helpers::run_ts_with_path(
            "import { Route } from './_authed/index';",
            &Check,
            "src/routes/__root.tsx",
        );
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }
}
