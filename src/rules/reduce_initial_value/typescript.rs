//! reduce-initial-value AST backend.
//!
//! Flags `.reduce(callback)` calls missing the initial-value second argument.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    // callee must be `*.reduce`
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "reduce" {
        return;
    }

    // arguments: must have exactly one argument (the callback, no initial value)
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let arg_count = args.named_child_count();
    if arg_count != 1 {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "reduce-initial-value".into(),
        message: "`.reduce()` without initial value \u{2014} throws on empty arrays.".into(),
        severity: Severity::Error,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_reduce_without_initial() {
        assert_eq!(run_on("const sum = arr.reduce((acc, x) => acc + x);").len(), 1);
    }

    #[test]
    fn flags_reduce_with_arrow_body() {
        assert_eq!(run_on("const r = items.reduce((a, b) => a.concat(b));").len(), 1);
    }

    #[test]
    fn allows_reduce_with_initial_value() {
        assert!(run_on("const sum = arr.reduce((acc, x) => acc + x, 0);").is_empty());
    }

    #[test]
    fn allows_reduce_with_object_initial() {
        assert!(run_on("const m = arr.reduce((acc, x) => ({ ...acc, [x]: 1 }), {});").is_empty());
    }
}
