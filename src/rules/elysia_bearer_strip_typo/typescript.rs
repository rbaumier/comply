//! elysia-bearer-strip-typo backend — flag .replace('Bearer', ...) without trailing space.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if !callee_text.ends_with(".replace") {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");

    // Look for 'Bearer' or "Bearer" not followed by a trailing space inside the literal.
    let bad = args_text.contains("'Bearer'")
        || args_text.contains("\"Bearer\"");

    if bad {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "elysia-bearer-strip-typo".into(),
            message: "`.replace('Bearer', '')` leaves a leading space in the token — use `'Bearer '` with trailing space.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_missing_space() {
        let src = "import { Elysia } from 'elysia';\nconst t = h.replace('Bearer', '');";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_with_trailing_space() {
        let src = "import { Elysia } from 'elysia';\nconst t = h.replace('Bearer ', '');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "const t = h.replace('Bearer', '');";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
