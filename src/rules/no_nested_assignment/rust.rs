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
        "assignment_expression" => return true,
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
}
