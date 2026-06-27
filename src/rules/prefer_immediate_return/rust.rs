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

/// Return the name bound by a whole-value `let X = …` binding, i.e. one that
/// binds the *entire* right-hand side to a single name:
/// - `identifier` — `let x = …`
/// - `mut_pattern` — `let mut x = …`
///
/// Returns `None` for destructuring patterns (`let (a, b) = …`,
/// `let (q, _) = …`, `let Foo { x } = …`) and reference patterns
/// (`let ref x = …`), which bind only *part* of the value or change its type:
/// inlining `let (q, _) = expr; q` to `expr` would return the whole tuple
/// instead of `q`. Only a whole-value binding is safe to inline.
fn extract_let_var_name<'a>(let_node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let pattern = let_node.child_by_field_name("pattern")?;
    match pattern.kind() {
        "identifier" => pattern.utf8_text(source).ok(),
        "mut_pattern" => {
            let mut cursor = pattern.walk();
            pattern
                .named_children(&mut cursor)
                .find(|child| child.kind() == "identifier")
                .and_then(|child| child.utf8_text(source).ok())
        }
        _ => None,
    }
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
    fn flags_mut_assign_then_tail_expr() {
        // `let mut x = …; x` binds the whole value to one name — still a
        // true positive. Negative control pinning the `mut` surface so the
        // wildcard-destructuring fix below does not regress it.
        let src = "fn f() -> i32 { let mut result = compute(); result }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn does_not_flag_destructuring_pattern() {
        // `let (a, b) = pair; return a;` — pattern is a tuple, skip.
        let src = "fn f() -> i32 { let (a, b) = pair(); return a; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_tuple_destructuring_with_wildcard() {
        // `let (q, _) = …; q` returns one tuple element, not the whole value:
        // inlining to `div_rem(a, b)` would change the type from `B` to
        // `(B, B)`. The wildcard `_` is an anonymous node, so the
        // `tuple_pattern` has a single named child (`q`) — the FP from #6285.
        let src = "fn div(a: B, b: B) -> B { let (q, _) = div_rem(a, b); q }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_tuple_destructuring_second_element() {
        // Mirror of the wildcard FP binding the second element.
        let src = "fn rem(a: B, b: B) -> B { let (_, r) = div_rem(a, b); r }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_ref_pattern() {
        // `let ref x = …; x` binds `x: &T` while the RHS is `T`; inlining to
        // the expression would change the returned type, so `ref_pattern` is
        // not a whole-value binding and must not be flagged.
        let src = "fn f() -> &i32 { let ref x = make(); x }";
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
