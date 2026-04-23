//! no-import-node-test backend — flag `import ... from 'node:test'`.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true if `spec` (quoted) equals `node:test`.
fn is_node_test(spec: &str) -> bool {
    let inner = spec.trim_matches(|c| c == '\'' || c == '"' || c == '`');
    inner == "node:test"
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "import_statement" { return; }

    let Some(src) = node.child_by_field_name("source") else { return };
    let text = src.utf8_text(source).unwrap_or("");
    if !is_node_test(text) { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-import-node-test".into(),
        message: "Importing from `node:test` mixes test runners; use vitest/jest APIs instead.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_default_node_test_import() {
        let d = run_on("import test from 'node:test';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("node:test"));
    }

    #[test]
    fn flags_named_node_test_import() {
        let d = run_on("import { describe, it } from 'node:test';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_double_quoted_node_test_import() {
        let d = run_on("import { test } from \"node:test\";");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_vitest_import() {
        assert!(run_on("import { describe, it } from 'vitest';").is_empty());
    }

    #[test]
    fn allows_jest_import() {
        assert!(run_on("import { jest } from '@jest/globals';").is_empty());
    }

    #[test]
    fn allows_other_node_builtin() {
        assert!(run_on("import { readFile } from 'node:fs';").is_empty());
    }
}
