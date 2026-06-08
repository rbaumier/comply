//! no-invariant-returns Rust backend.
//!
//! Walks `function_item` nodes, collects the `return_expression` nodes
//! belonging directly to the function body plus the function block's tail
//! expression (Rust's implicit return), and flags the function when every
//! resulting value is the same literal.
//!
//! Nested `function_item` and `closure_expression` subtrees are skipped so
//! an inner closure's `return` is not attributed to the outer function.

use crate::diagnostic::{Diagnostic, Severity};

/// Recursively scan `node`'s subtree for `return_expression` nodes,
/// stopping at nested function/closure boundaries so inner returns
/// are attributed to the inner function only.
fn collect_returns<'t>(node: tree_sitter::Node<'t>, out: &mut Vec<tree_sitter::Node<'t>>) {
    let count = node.child_count();
    for i in 0..count {
        let Some(child) = node.child(i) else { continue };
        match child.kind() {
            "function_item" | "closure_expression" => {
                // Skip — its returns belong to that inner function.
            }
            "return_expression" => {
                out.push(child);
            }
            _ => collect_returns(child, out),
        }
    }
}

/// Extract a normalized literal text from a `return_expression` value, or
/// from a tail expression. Returns `None` for non-literals.
fn literal_text<'a>(value: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let kind = value.kind();
    let text = value.utf8_text(source).ok()?.trim();
    match kind {
        "integer_literal" | "float_literal" | "string_literal" | "char_literal"
        | "boolean_literal" | "raw_string_literal" => Some(text),
        // `None` shows up as a regular identifier in expression position.
        "identifier" if text == "None" => Some(text),
        _ => None,
    }
}

/// Pull the value of a `return_expression`, if any (bare `return` has no
/// child). Returns `None` for bare returns and non-literal values.
fn return_value_literal<'a>(ret: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let value = ret.named_child(0)?;
    literal_text(value, source)
}

/// True if `block` is a `block` node whose final child is an expression
/// (Rust's implicit return). Returns the expression node when it is.
fn block_tail_expression<'t>(block: tree_sitter::Node<'t>) -> Option<tree_sitter::Node<'t>> {
    if block.kind() != "block" {
        return None;
    }
    let last = block.named_child(block.named_child_count().checked_sub(1)?)?;
    // The block grammar tags the trailing expression node as the last named
    // child; statements end with `;` and are nodes like `let_declaration`,
    // `expression_statement`. An expression node is anything else with a
    // value-producing kind.
    let kind = last.kind();
    if kind == "let_declaration" || kind == "expression_statement" {
        return None;
    }
    Some(last)
}

crate::ast_check! { on ["function_item"] => |node, source, ctx, diagnostics|
    let Some(body) = node.child_by_field_name("body") else { return };

    let mut returns: Vec<tree_sitter::Node> = Vec::new();
    collect_returns(body, &mut returns);

    let mut literals: Vec<&str> = Vec::new();
    for ret in &returns {
        let Some(lit) = return_value_literal(*ret, source) else {
            return; // Non-literal return — can't prove invariance.
        };
        literals.push(lit);
    }

    if let Some(tail) = block_tail_expression(body) {
        let Some(lit) = literal_text(tail, source) else {
            // Tail is a non-literal expression — bail out unless there are
            // no return statements at all (in which case we have nothing).
            return;
        };
        literals.push(lit);
    }

    if literals.len() < 2 {
        return;
    }

    let first = literals[0];
    if !literals.iter().all(|l| *l == first) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-invariant-returns".into(),
        message: "Function always returns the same literal value \u{2014} consider using a constant instead.".into(),
        severity: Severity::Warning,
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
    fn flags_invariant_true() {
        let src = r#"
fn is_enabled(x: i32) -> bool {
    if x > 0 {
        return true;
    }
    return true;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_different_returns() {
        let src = r#"
fn is_positive(n: i32) -> bool {
    if n > 0 {
        return true;
    }
    return false;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_single_return() {
        let src = r#"
fn get_value() -> i32 {
    return 42;
}
"#;
        assert!(run_on(src).is_empty());
    }
}
