//! zod-prefer-strict-object backend — flag `.strict()` chained after `z.object(...)`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" { return; }
    let Some(prop) = func.child_by_field_name("property") else { return };
    let Ok(prop_text) = prop.utf8_text(source) else { return };
    if prop_text != "strict" { return; }

    // The receiver must itself be a call_expression whose function is `z.object`.
    let Some(receiver) = func.child_by_field_name("object") else { return };
    if receiver.kind() != "call_expression" { return; }
    let Some(recv_func) = receiver.child_by_field_name("function") else { return };
    let Ok(recv_text) = recv_func.utf8_text(source) else { return };
    if recv_text != "z.object" { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`z.object({...}).strict()` is deprecated in Zod v4 — \
                  use `z.strictObject({...})` instead.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_object_strict_chain() {
        assert_eq!(run("const S = z.object({ a: z.string() }).strict();").len(), 1);
    }

    #[test]
    fn allows_strict_object_factory() {
        assert!(run("const S = z.strictObject({ a: z.string() });").is_empty());
    }

    #[test]
    fn ignores_bare_object() {
        assert!(run("const S = z.object({ a: z.string() });").is_empty());
    }
}
