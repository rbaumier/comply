//! no-nested-assignment Rust backend.
//!
//! Flags an assignment that is itself the condition of an `if`/`while`
//! (`if x = 5 {}`). In Rust this exact form is uncompilable — assignment
//! evaluates to `()`, not `bool` — so it can only appear as a typo, never as
//! working code. The walk deliberately stops at any `block` or
//! `closure_expression` it enters: those open a new value-yielding scope, and
//! an assignment inside one is the idiomatic assign-then-yield block
//! (`{ x = ...; x }`) or a closure mutation, not a condition-level `=`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["if_expression", "while_expression"] => |node, source, _ctx, diagnostics|
match node.kind() {
        "if_expression" | "while_expression" => {}
        _ => return,
    }
    let Some(condition) = node.child_by_field_name("condition") else {
        return;
    };
    if contains_assignment(condition) {
        let pos = condition.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&_ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-nested-assignment".into(),
            message: "Assignment inside a condition \u{2014} likely a bug, use `==` for comparison or move the assignment out.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

fn contains_assignment(node: tree_sitter::Node) -> bool {
    match node.kind() {
        // tree-sitter-rust splits the `..=` inclusive-range operator into a
        // `..` range plus a trailing `= N`, synthesizing a spurious
        // `assignment_expression` whose `left` is a `range_expression` (e.g.
        // `f(..=16)` or `arr[..=n]`). A real Rust assignment targets a place
        // expression, never a range, so only flag when `left` is not a range.
        "assignment_expression" => {
            let left_is_range = node
                .child_by_field_name("left")
                .is_some_and(|left| left.kind() == "range_expression");
            return !left_is_range;
        }
        // A `block` or `closure_expression` opens a new value-yielding scope;
        // an assignment inside it is the assign-then-yield idiom or a closure
        // mutation, not a condition-level `=`. In Rust the bare `if x = 5 {}`
        // form is a type error (assignment is `()`, not `bool`) and never
        // compiles, so every assignment reachable under a condition lives
        // inside such a scope.
        "block" | "closure_expression" => return false,
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if contains_assignment(child) {
            return true;
        }
    }
    false
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn allows_equality_check() {
        assert!(run_on("fn f() { if x == 10 {} }").is_empty());
    }

    #[test]
    fn allows_comparison() {
        assert!(run_on("fn f() { if x <= 10 {} }").is_empty());
    }

    // Regression for #3895: an assign-then-yield block inside the condition is
    // the deliberate `{ x = ...; x }` idiom, not a condition-level `=`.
    #[test]
    fn allows_assign_then_yield_block_in_condition() {
        let src = "fn f() { let mut h = false; if (a() || { h = b(); h }) && c() { } }";
        assert!(run_on(src).is_empty());
    }

    // Regression for #3895: a mutation inside a closure passed to a combinator,
    // itself inside the `match` that is the condition, is a closure mutation.
    #[test]
    fn allows_closure_mutation_in_condition() {
        let src = "fn f() { if match r { X => step(|cursor| { let mut cursor = begin; cursor = rest; true }), _ => false } { } }";
        assert!(run_on(src).is_empty());
    }

    // Load-bearing guard: a bare assignment that IS the condition is the one
    // theoretically-flaggable case and must still fire.
    #[test]
    fn flags_bare_assignment_in_condition() {
        assert_eq!(run_on("fn f() { if x = 5 { } }").len(), 1);
    }

    // Regression for #5362: `..=N` passed as a call argument is misparsed as an
    // `assignment_expression` over a `range_expression` left, not a real `=`.
    #[test]
    fn allows_inclusive_range_arg_in_comparison() {
        assert!(run_on("fn f() { if web_rng.usize(..=16) == 16 { } }").is_empty());
    }

    // Sibling shapes around #5362 that must also stay clean.
    #[test]
    fn allows_inclusive_range_in_index_comparison() {
        assert!(run_on("fn f() { if arr[..=n] == y { } }").is_empty());
    }

    #[test]
    fn allows_inclusive_range_in_macro() {
        assert!(run_on("fn f() { if matches!(x, ..=16) { } }").is_empty());
    }

    // A genuine nested assignment used inside a comparison must still fire even
    // though it sits under a `binary_expression`/`parenthesized_expression`.
    #[test]
    fn flags_nested_assignment_in_comparison() {
        assert_eq!(run_on("fn f() { if (x = compute()) != 0 { } }").len(), 1);
    }

    #[test]
    fn flags_nested_assignment_in_while_comparison() {
        assert_eq!(run_on("fn f() { while (n = next()) > 0 { } }").len(), 1);
    }
}
