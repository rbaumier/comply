//! prefer-immediate-return Rust backend.
//!
//! Flag `let x = expr; return x;` or `let x = expr; x` (tail expression)
//! that should be simplified to `return expr;` or just `expr`.
//!
//! ## Why this rule was rewritten
//!
//! The previous implementation was a text scanner that walked pairs
//! of consecutive non-blank lines and matched them lexically against
//! `let x = …` / `return x;` shapes. It produced a false positive on
//! multi-line method chains like
//!
//! ```ignore
//! let mut parser = tree_sitter::Parser::new();
//! parser
//!     .set_language(&…)
//!     .unwrap();
//! ```
//!
//! where the second non-blank line is `parser` — the start of a
//! chained call, not a tail expression. The user's reported FP.
//!
//! ## How the new rule works
//!
//! Walks tree-sitter `block` nodes and looks at the consecutive
//! *named children* of each block, not at consecutive source lines:
//!
//! 1. `child[i]` must be a `let_declaration` whose pattern is a
//!    single simple identifier `X` (skips tuple / struct /
//!    destructuring patterns).
//! 2. `child[i+1]` must be one of:
//!    - `expression_statement` wrapping `return_expression`
//!      whose value is exactly `identifier X`, OR
//!    - the block's tail expression: bare `identifier X`.
//!
//! Anything else — a method call on `X`, another statement, a
//! different variable returned — breaks the pattern and the pair
//! is not flagged. The multi-line method chain FP disappears
//! because the second named child is an `expression_statement`
//! containing a `call_expression`, not `return_expression` and
//! not a bare identifier.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["block"])
    }

    fn visit_node(
        &self,
        block: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let mut cursor = block.walk();
        let children: Vec<_> = block.named_children(&mut cursor).collect();
        for i in 0..children.len().saturating_sub(1) {
            let let_node = children[i];
            let next_node = children[i + 1];
            if let_node.kind() != "let_declaration" {
                continue;
            }
            let Some(var_name) = extract_let_var_name(let_node, source_bytes) else {
                continue;
            };
            if !next_is_return_or_tail_of(next_node, source_bytes, var_name) {
                continue;
            }
            let pos = let_node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "prefer-immediate-return".into(),
                message: format!(
                    "Variable `{var_name}` is assigned and immediately \
                     returned — return the expression directly."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

/// Return the single simple identifier bound by `let X = …`. Returns
/// `None` for destructuring patterns (`let (a, b) = …`, `let Foo { x } = …`).
fn extract_let_var_name<'a>(let_node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let pattern = let_node.child_by_field_name("pattern")?;
    first_simple_identifier(pattern, source)
}

fn first_simple_identifier<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    if node.kind() == "identifier" {
        return node.utf8_text(source).ok();
    }
    let mut cursor = node.walk();
    let children: Vec<_> = node.named_children(&mut cursor).collect();
    if children.len() != 1 {
        return None;
    }
    first_simple_identifier(children[0], source)
}

/// True if `node` is exactly `return X;` or the block's tail
/// `X` where `X` is the target variable.
fn next_is_return_or_tail_of(node: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    // Tail expression: bare identifier, last named child of the block.
    if node.kind() == "identifier" {
        return node.utf8_text(source).ok() == Some(name);
    }
    // Statement form: `return X;` lives inside an expression_statement.
    if node.kind() == "expression_statement" {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "return_expression" {
                return return_value_is_identifier(child, source, name);
            }
        }
        return false;
    }
    // Direct form: `return_expression` as a child of the block (rare).
    if node.kind() == "return_expression" {
        return return_value_is_identifier(node, source, name);
    }
    false
}

fn return_value_is_identifier(ret_node: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    let mut cursor = ret_node.walk();
    let Some(value) = ret_node.named_children(&mut cursor).next() else {
        return false;
    };
    value.kind() == "identifier" && value.utf8_text(source).ok() == Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_assign_then_return() {
        let src = "fn f() -> i32 { let result = compute(); return result; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_assign_then_tail_expr() {
        let src = "fn f() -> i32 { let result = compute(); result }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_assign_used_later() {
        let src = "fn f() -> i32 { let result = compute(); println!(\"{}\", result); result }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_different_variable_returned() {
        let src = "fn f() -> i32 { let result = compute(); return other; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_method_chain_on_next_line() {
        // The user's exact FP: `parser` on the next line is the start
        // of a multi-line method chain, not a tail expression.
        let src = r#"
            fn run() {
                let mut parser = tree_sitter::Parser::new();
                parser
                    .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
                    .unwrap();
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_destructuring_pattern() {
        // `let (a, b) = pair; return a;` — pattern is a tuple, skip.
        let src = "fn f() -> i32 { let (a, b) = pair(); return a; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_when_variable_is_used_in_method_call_before_return() {
        let src = r#"
            fn f() -> MyType {
                let mut x = make();
                x.configure();
                x
            }
        "#;
        assert!(run_on(src).is_empty());
    }
}
