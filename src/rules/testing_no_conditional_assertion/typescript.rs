//! testing-no-conditional-assertion backend — flag `expect(...)` calls
//! whose closest enclosing statement node is an `if_statement` body, and
//! which also live inside a `test(...)` / `it(...)` callback.
//!
//! Why: `if (cond) expect(x).toBe(y)` passes trivially whenever `cond`
//! is false. The test gives a green checkmark while asserting nothing.
//! Make assertions unconditional.

use crate::diagnostic::{Diagnostic, Severity};

/// Is `func` a bare `expect` identifier (top-level `expect(x).toBe(...)`
/// resolves via the inner `expect(x)` call — that's what we flag)?
fn is_bare_expect_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(func) = node.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "identifier" {
        return false;
    }
    func.utf8_text(source).unwrap_or("") == "expect"
}

/// Walk ancestors looking for (a) an if_statement this node is inside the
/// `consequence`/`alternative` of, and (b) a `test(...)` / `it(...)` call
/// wrapping the whole thing. Both must be present.
fn enclosing_if_and_test(mut node: tree_sitter::Node, source: &[u8]) -> (bool, bool) {
    let mut in_if_body = false;
    let mut in_test = false;
    while let Some(parent) = node.parent() {
        if !in_if_body && parent.kind() == "if_statement" {
            // Only count if `node` is in the consequence or alternative,
            // not the condition expression.
            let cond = parent.child_by_field_name("condition");
            if cond.map(|c| c.id()) != Some(node.id()) {
                in_if_body = true;
            }
        }
        if !in_test
            && parent.kind() == "call_expression"
            && let Some(func) = parent.child_by_field_name("function")
            && func.kind() == "identifier"
        {
            let n = func.utf8_text(source).unwrap_or("");
            if matches!(n, "test" | "it") {
                in_test = true;
            }
        }
        if in_if_body && in_test {
            return (true, true);
        }
        node = parent;
    }
    (in_if_body, in_test)
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if !is_bare_expect_call(node, source) { return; }
    let (in_if, in_test) = enclosing_if_and_test(node, source);
    if in_if && in_test {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "expect(...) inside an if-branch silently skips when the branch is not taken — make the assertion unconditional.".into(),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_expect_inside_if_in_test() {
        let src = "test('a', () => {\n\
                     if (x > 0) { expect(x).toBeGreaterThan(0); }\n\
                   });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_expect_inside_else_in_it() {
        let src = "it('a', () => {\n\
                     if (ok) { doThing(); } else { expect(ok).toBe(true); }\n\
                   });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_unconditional_expect_in_test() {
        let src = "test('a', () => { expect(x).toBe(1); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_if_expect_outside_test() {
        let src = "function helper(x) { if (x) expect(x).toBe(1); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_expect_whose_argument_contains_conditional() {
        // The call itself is unconditional; the ternary lives in the arg.
        let src = "test('a', () => { expect(x ? 1 : 2).toBe(1); });";
        assert!(run(src).is_empty());
    }
}
