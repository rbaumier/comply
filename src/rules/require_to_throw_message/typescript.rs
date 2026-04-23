//! require-to-throw-message backend — flag `.toThrow()` / `.toThrowError()`
//! called without an expected-message argument.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(prop) = callee.child_by_field_name("property") else { return };
    let name = prop.utf8_text(source).unwrap_or("");
    if name != "toThrow" && name != "toThrowError" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    // `arguments` node contains parentheses + commas; count named children only.
    if args.named_child_count() != 0 {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "require-to-throw-message",
        "Provide expected error message to toThrow().".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_empty_to_throw() {
        let d = run_on("expect(() => foo()).toThrow();");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "require-to-throw-message");
    }

    #[test]
    fn flags_empty_to_throw_error() {
        let d = run_on("expect(() => foo()).toThrowError();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_to_throw_with_string() {
        assert!(run_on("expect(() => foo()).toThrow('boom');").is_empty());
    }

    #[test]
    fn allows_to_throw_error_with_regex() {
        assert!(run_on("expect(() => foo()).toThrowError(/boom/);").is_empty());
    }

    #[test]
    fn ignores_unrelated_member_calls() {
        assert!(run_on("expect(x).toBe();").is_empty());
    }
}
