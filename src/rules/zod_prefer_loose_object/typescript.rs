//! zod-prefer-loose-object backend — flag `.passthrough()` chained after `z.object(...)`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" { return; }
    let Some(prop) = func.child_by_field_name("property") else { return };
    let Ok(prop_text) = prop.utf8_text(source) else { return };
    if prop_text != "passthrough" { return; }

    let Some(receiver) = func.child_by_field_name("object") else { return };
    if receiver.kind() != "call_expression" { return; }
    let Some(recv_func) = receiver.child_by_field_name("function") else { return };
    let Ok(recv_text) = recv_func.utf8_text(source) else { return };
    if recv_text != "z.object" { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`z.object({...}).passthrough()` is deprecated in Zod v4 — \
                  use `z.looseObject({...})` instead.".into(),
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
    fn flags_passthrough_chain() {
        assert_eq!(run("const S = z.object({ a: z.string() }).passthrough();").len(), 1);
    }

    #[test]
    fn allows_loose_object_factory() {
        assert!(run("const S = z.looseObject({ a: z.string() });").is_empty());
    }

    #[test]
    fn ignores_bare_object() {
        assert!(run("const S = z.object({ a: z.string() });").is_empty());
    }
}
