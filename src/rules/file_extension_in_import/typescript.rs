//! file-extension-in-import backend — flag relative imports missing a file extension.
//!
//! Walks the program's top-level `import_statement` and `export_statement`
//! (re-export) nodes, extracts the source specifier, and flags relative
//! specifiers (`./` or `../`) that do not end in a known extension and are
//! not directory-style imports (trailing `/` or `/index`).

use crate::diagnostic::{Diagnostic, Severity};

const KNOWN_EXTENSIONS: &[&str] = &[
    ".js", ".ts", ".tsx", ".jsx", ".mjs", ".cjs", ".mts", ".cts", ".json",
    ".css", ".scss", ".less", ".svg", ".png", ".vue", ".svelte",
];

fn has_known_extension(spec: &str) -> bool {
    KNOWN_EXTENSIONS.iter().any(|ext| spec.ends_with(ext))
}

fn is_directory_import(spec: &str) -> bool {
    spec.ends_with('/') || spec.ends_with("/index")
}

fn is_relative(spec: &str) -> bool {
    spec.starts_with("./") || spec.starts_with("../")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let kind = child.kind();
        if kind != "import_statement" && kind != "export_statement" {
            continue;
        }
        let Some(source_node) = child.child_by_field_name("source") else {
            continue;
        };
        let Ok(raw) = std::str::from_utf8(&source[source_node.byte_range()]) else {
            continue;
        };
        let spec = raw.trim_matches(|c| c == '"' || c == '\'' || c == '`');

        if !is_relative(spec) {
            continue;
        }
        if has_known_extension(spec) {
            continue;
        }
        if is_directory_import(spec) {
            continue;
        }

        let pos = source_node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "file-extension-in-import".into(),
            message: format!(
                "Relative import `{spec}` is missing a file extension. Add an explicit extension (e.g. `.js`, `.ts`) for ESM compatibility.",
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_relative_import_without_extension() {
        let d = run_on("import { foo } from './utils';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_relative_import_with_extension() {
        assert!(run_on("import { foo } from './utils.js';").is_empty());
    }

    #[test]
    fn allows_ts_extension() {
        assert!(run_on("import { foo } from './utils.ts';").is_empty());
    }

    #[test]
    fn skips_bare_specifier() {
        assert!(run_on("import React from 'react';").is_empty());
    }

    #[test]
    fn flags_parent_relative_without_extension() {
        let d = run_on("import { bar } from '../helpers/bar';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_json_extension() {
        assert!(run_on("import data from './config.json';").is_empty());
    }

    #[test]
    fn flags_reexport_without_extension() {
        let d = run_on("export { foo } from './utils';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn skips_node_protocol() {
        assert!(run_on("import fs from 'node:fs';").is_empty());
    }

    #[test]
    fn skips_scoped_bare_specifier() {
        assert!(run_on("import x from '@scope/pkg';").is_empty());
    }

    #[test]
    fn skips_directory_import_trailing_slash() {
        assert!(run_on("import x from './components/';").is_empty());
    }

    #[test]
    fn skips_directory_import_index() {
        assert!(run_on("import x from './components/index';").is_empty());
    }

    #[test]
    fn allows_tsx_extension() {
        assert!(run_on("import Btn from './Button.tsx';").is_empty());
    }

    #[test]
    fn skips_dynamic_import() {
        // Dynamic imports are call_expression nodes, not import_statement.
        assert!(run_on("const m = import('./utils');").is_empty());
    }
}
