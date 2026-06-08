//! rust-repeated-string-concat-in-loop backend.
//!
//! For each loop kind (`for_expression`, `while_expression`, `loop_expression`)
//! walk the body looking for either:
//! - an `assignment_expression` whose right-hand side is a `binary_expression`
//!   with `+` and whose left-hand side is the same identifier as the assignment
//!   target (the `s = s + x` shape), or
//! - a `compound_assignment_expr` with `+=` (the `s += x` shape).
//!
//! `push_str` in a loop is NOT flagged — it is the recommended fix.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["for_expression", "while_expression", "loop_expression"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source = ctx.source.as_bytes();
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        if let Some(culprit) = find_concat(body, source) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &culprit,
                "rust-repeated-string-concat-in-loop",
                "string concatenation inside a loop reallocates per \
                 iteration. Pre-size a `String` with `with_capacity` and \
                 `push_str` into it, or collect into `Vec<String>` then \
                 `.join(\"\")`."
                    .into(),
                Severity::Warning,
            ));
        }
    }
}

fn find_concat<'a>(node: tree_sitter::Node<'a>, source: &[u8]) -> Option<tree_sitter::Node<'a>> {
    let mut stack = vec![node];
    while let Some(cur) = stack.pop() {
        if is_self_concat_assign(cur, source) {
            return Some(cur);
        }
        if is_compound_concat_assign(cur, source) {
            return Some(cur);
        }
        let mut cursor = cur.walk();
        for child in cur.children(&mut cursor) {
            // Don't recurse into nested loops — they get their own
            // diagnostic from the outer walker.
            if matches!(
                child.kind(),
                "for_expression" | "while_expression" | "loop_expression"
            ) {
                continue;
            }
            stack.push(child);
        }
    }
    None
}

/// True if `node` is `s += …` (compound assignment with `+=`) where the
/// right-hand side is not a numeric literal (avoids flagging `i += 1`).
fn is_compound_concat_assign(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "compound_assignment_expr" {
        return false;
    }
    let Some(op) = node.child_by_field_name("operator") else {
        return false;
    };
    if op.utf8_text(source).map(|t| t != "+=").unwrap_or(true) {
        return false;
    }
    let Some(rhs) = node.child_by_field_name("right") else {
        return false;
    };
    !matches!(rhs.kind(), "integer_literal" | "float_literal")
}

/// True if `node` is `s = s + …` for the same identifier `s`.
fn is_self_concat_assign(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "assignment_expression" {
        return false;
    }
    let Some(lhs) = node.child_by_field_name("left") else {
        return false;
    };
    let Some(rhs) = node.child_by_field_name("right") else {
        return false;
    };
    if rhs.kind() != "binary_expression" {
        return false;
    }
    let Some(op) = rhs.child_by_field_name("operator") else {
        return false;
    };
    if op.utf8_text(source).map(|t| t != "+").unwrap_or(true) {
        return false;
    }
    let Some(rhs_left) = rhs.child_by_field_name("left") else {
        return false;
    };
    let Ok(lhs_text) = lhs.utf8_text(source) else {
        return false;
    };
    let Ok(rhs_left_text) = rhs_left.utf8_text(source) else {
        return false;
    };
    lhs_text == rhs_left_text
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
    fn flags_self_concat_in_for() {
        let src = r#"fn f() { let mut s = String::new(); for x in v { s = s + "x"; } }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_self_concat_in_while() {
        let src = r#"fn f() { let mut s = String::new(); while cond { s = s + "x"; } }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_plus_eq_in_loop() {
        let src = r#"fn f() { let mut s = String::new(); loop { s += "x"; break; } }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_push_str_in_for() {
        let src = r#"fn f() { let mut s = String::new(); for x in v { s.push_str("x"); } }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_push_str_in_loop() {
        let src = r#"fn f() { let mut s = String::new(); loop { s.push_str("x"); break; } }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_push_str_outside_loop() {
        let src = r#"fn f() { let mut s = String::new(); s.push_str("x"); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_integer_counter_in_loop() {
        let src = r#"fn f() { let mut i = 0; for _ in v { i += 1; } }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_with_capacity_pre_loop() {
        let src = r#"fn f() { let s = String::with_capacity(100); for _ in v { let _ = &s; } }"#;
        assert!(run_on(src).is_empty());
    }
}
