//! elysia-eden-null-body backend — flag `undefined` body argument in Eden mutations.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let prop_text = prop.utf8_text(source).unwrap_or("");
    if !matches!(prop_text, "post" | "put" | "patch") {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    // Find first argument (skipping parens/commas).
    let mut first_arg = None;
    for i in 0..args.child_count() {
        let Some(child) = args.child(i) else { continue };
        let kind = child.kind();
        if kind == "(" || kind == "," || kind == ")" {
            continue;
        }
        first_arg = Some(child);
        break;
    }
    let Some(arg) = first_arg else { return };
    if arg.utf8_text(source).unwrap_or("") != "undefined" {
        return;
    }

    let pos = arg.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-eden-null-body".into(),
        message: "Eden mutation called with `undefined` body — pass `null` instead so the request serializes correctly.".into(),
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
    fn flags_undefined_post_body() {
        let src = "import { treaty } from '@elysiajs/eden';\nawait treaty.users.post(undefined, { headers: {} });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_undefined_patch_body() {
        let src = "import { treaty } from '@elysiajs/eden';\nawait api.users({ id: 1 }).patch(undefined);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_null_body() {
        let src = "import { treaty } from '@elysiajs/eden';\nawait treaty.users.post(null);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_payload_body() {
        let src = "import { treaty } from '@elysiajs/eden';\nawait treaty.users.post({ name: 'a' });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_eden_files() {
        let src = "fetch.post(undefined);";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
