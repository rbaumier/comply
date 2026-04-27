//! elysia-import-t-not-typebox backend — flag direct TypeBox imports in Elysia files.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") || !ctx.source.contains("@sinclair/typebox") {
        return;
    }

    let text = node.utf8_text(source).unwrap_or("");
    if !text.contains("@sinclair/typebox") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-import-t-not-typebox".into(),
        message: "Import `t` from `elysia` instead of `Type` from `@sinclair/typebox` — Elysia ships augmented validators.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_typebox_import_in_elysia_file() {
        let src = "import { Elysia } from 'elysia';\nimport { Type } from '@sinclair/typebox';\n";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_t_from_elysia() {
        let src = "import { Elysia, t } from 'elysia';\n";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_typebox_outside_elysia_files() {
        let src = "import { Type } from '@sinclair/typebox';\n";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
