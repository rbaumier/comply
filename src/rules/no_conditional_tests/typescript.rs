//! no-conditional-tests backend — flag `describe`/`test`/`it` calls whose
//! ancestor chain contains an `if_statement`, `ternary_expression`, or
//! `switch_case`. Conditional test definitions make the suite
//! non-deterministic; prefer `.skip`/`.skipIf` instead.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_FNS: &[&str] = &["describe", "test", "it"];

fn callee_is_test_fn(name: &str) -> bool {
    // Accept bare `test`, `it`, `describe` and member-access variants like
    // `test.each`, `describe.only` — the base identifier is what matters.
    let base = name.split('.').next().unwrap_or(name);
    TEST_FNS.contains(&base)
}

fn has_conditional_ancestor(node: tree_sitter::Node) -> bool {
    let mut cur = node.parent();
    while let Some(parent) = cur {
        match parent.kind() {
            "if_statement" | "ternary_expression" | "switch_case" => return true,
            _ => {}
        }
        cur = parent.parent();
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    let Some(function) = node.child_by_field_name("function") else { return };
    // Only flag direct calls — `test.each([1])('a', ...)` has an outer
    // `call_expression` whose function child is another call; we only want
    // to flag the inner `test.each([1])` once.
    if !matches!(function.kind(), "identifier" | "member_expression") {
        return;
    }
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if !callee_is_test_fn(name) {
        return;
    }
    if !has_conditional_ancestor(node) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "no-conditional-tests",
        "Don't conditionally define tests, use test.skip or describe.skip".into(),
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
    fn flags_test_inside_if() {
        let src = "if (flag) { test('a', () => {}); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_describe_inside_ternary() {
        let src = "flag ? describe('a', () => {}) : null;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_it_inside_switch_case() {
        let src = "switch (x) { case 1: it('a', () => {}); break; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_test_each_inside_if() {
        let src = "if (flag) { test.each([1])('a', () => {}); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_top_level_test() {
        let src = "test('a', () => {});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_describe_with_inner_if() {
        let src = "describe('a', () => { if (flag) { doStuff(); } });";
        assert!(run_on(src).is_empty());
    }
}
