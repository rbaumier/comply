//! elysia-derive-validated-data backend — flag `.derive(` callbacks reading body/params/query.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if !callee_text.ends_with(".derive") {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");

    let touches_validated = args_text.contains("body")
        || args_text.contains("params")
        || args_text.contains("query");
    if !touches_validated {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-derive-validated-data".into(),
        message: "`.derive()` reads `body`/`params`/`query` before validation — use `.resolve()` to access validated data.".into(),
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
    fn flags_derive_reading_body() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().derive(({ body }) => ({ b: body }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_derive_reading_only_headers() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().derive(({ headers }) => ({ token: headers.authorization }));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "obj.derive(({ body }) => body);";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
