//! elysia-macro-named-inference backend — flag `.macro({ ... })` bulk form.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if !callee_text.ends_with(".macro") {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    // Walk first non-trivial child of arguments to find the first argument's kind.
    let mut cursor = args.walk();
    let mut first_arg_kind: Option<&str> = None;
    for child in args.children(&mut cursor) {
        if child.is_named() {
            first_arg_kind = Some(child.kind());
            break;
        }
    }
    let Some(kind) = first_arg_kind else { return };
    if kind != "object" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-macro-named-inference".into(),
        message: "`.macro({ ... })` bulk form blocks cross-macro inference — use `.macro('name', { ... })`.".into(),
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
    fn flags_object_first_arg() {
        let src =
            "import { Elysia } from 'elysia';\nnew Elysia().macro({ isAuth: { resolve() {} } });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_named_form() {
        let src =
            "import { Elysia } from 'elysia';\nnew Elysia().macro('isAuth', { resolve() {} });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "obj.macro({ isAuth: {} });";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
