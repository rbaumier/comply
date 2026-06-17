//! no-collapsible-if Rust backend.
//!
//! Flag `if a { if b { } }` that should be `if a && b { }`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["if_expression"] => |node, _source, ctx, diagnostics|
    // Skip else-if arms: merging them would harm readability of control-flow chains.
    if let Some(parent) = node.parent()
        && parent.kind() == "else_clause" {
            return;
    }

    // The outer if must NOT have an else clause.
    if node.child_by_field_name("alternative").is_some() {
        return;
    }

    // Get the consequence (body) of the outer if.
    let Some(body) = node.child_by_field_name("consequence") else { return };

    if body.kind() != "block" {
        return;
    }

    // The body must contain exactly one named child.
    if body.named_child_count() != 1 {
        return;
    }

    let Some(only_child) = body.named_child(0) else { return };

    // In Rust tree-sitter, the if_expression is wrapped in expression_statement.
    let inner_if = if only_child.kind() == "expression_statement" {
        only_child.named_child(0)
    } else {
        Some(only_child)
    };
    let Some(inner_if) = inner_if else { return };

    // That single child must be an if_expression.
    if inner_if.kind() != "if_expression" {
        return;
    }

    // The inner if must also NOT have an else clause.
    if inner_if.child_by_field_name("alternative").is_some() {
        return;
    }

    // Skip when either condition is an `if let` (a `let_condition`): pattern
    // bindings cannot be combined with `&&` (only via a let-chain, which is
    // not the suggested `a && b` form and requires Rust 1.88+ / edition 2024).
    let outer_cond = node.child_by_field_name("condition");
    let inner_cond = inner_if.child_by_field_name("condition");
    if outer_cond.is_some_and(|c| c.kind() == "let_condition")
        || inner_cond.is_some_and(|c| c.kind() == "let_condition")
    {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-collapsible-if".into(),
        message: "Nested `if` without `else` can be merged into a single `if a && b`.".into(),
        severity: Severity::Error,
        span: None,
    });
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
    fn flags_nested_if() {
        let src = r#"
fn f() {
    if a {
        if b {
            do_something();
        }
    }
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_outer_if_with_else() {
        let src = r#"
fn f() {
    if a {
        if b {
            do_something();
        }
    } else {
        do_other();
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_inner_if_with_else() {
        let src = r#"
fn f() {
    if a {
        if b {
            do_something();
        } else {
            do_other();
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_else_if_with_nested_if() {
        let src = r#"
fn f() {
    if a { do_a(); }
    else if b {
        if c {
            do_c();
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_multiple_statements_in_body() {
        let src = r#"
fn f() {
    if a {
        setup();
        if b {
            do_something();
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_nested_if_let() {
        let src = r#"
fn f(&mut self) {
    if let Some(ch) = self.ch.take() {
        if let Some(buf) = &mut self.raw_buffer {
            buf.push(ch);
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_outer_plain_inner_if_let() {
        let src = r#"
fn f() {
    if cond {
        if let Some(v) = y {
            do_something(v);
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_outer_if_let_inner_plain() {
        let src = r#"
fn f() {
    if let Some(v) = x {
        if cond {
            do_something(v);
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }
}
