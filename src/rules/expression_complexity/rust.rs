//! expression-complexity Rust backend.
//!
//! Flags a logical chain that joins its operands with 4+ `&&` / `||`
//! operators and still holds an operand that carries no name. Rust has no
//! ternary `?` or `??`, so only `&&` and `||` count.
//!
//! The unit is the chain, not the source line: the count is taken over the
//! outermost `binary_expression` of the chain, so a chain wrapped one operand
//! per line counts like the single-line form, and the diagnostic is anchored
//! on the expression rather than on the left margin.
//!
//! Operators are read off `binary_expression` nodes, so `&&` / `||` bytes
//! that carry no operator — inside a string literal, a comment, or a macro
//! token tree — never count.
//!
//! The count stops at nested callables (`closure_expression`,
//! `function_item`): a lambda predicate holds a chain of its own, measured
//! separately as its own root, and must not inflate the enclosing chain.

use crate::diagnostic::{Diagnostic, Severity};

const LOGICAL_OPS: &[&str] = &["&&", "||"];
const CALLABLE_BOUNDARIES: &[&str] = &["closure_expression", "function_item"];

/// Node kinds an operand can wear while still belonging to the enclosing
/// chain: `(a && b) && c` and `!(a && b) && c` are each one chain.
const TRANSPARENT_WRAPPERS: &[&str] = &["parenthesized_expression", "unary_expression"];

fn is_logical_expression(node: tree_sitter::Node, source: &[u8]) -> bool {
    node.kind() == "binary_expression"
        && node
            .child_by_field_name("operator")
            .and_then(|op| op.utf8_text(source).ok())
            .is_some_and(|op| LOGICAL_OPS.contains(&op))
}

fn count_logical_ops(node: tree_sitter::Node, source: &[u8]) -> usize {
    let mut count = 0;
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        if CALLABLE_BOUNDARIES.contains(&current.kind()) {
            continue;
        }
        if is_logical_expression(current, source) {
            count += 1;
        }
        let mut cursor = current.walk();
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
    count
}

/// Returns the single expression a `parenthesized_expression` or a
/// `unary_expression` wraps, skipping the `line_comment` / `block_comment`
/// nodes tree-sitter also reports as named children.
fn inner_operand(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    (0..node.named_child_count())
        .filter_map(|i| node.named_child(i))
        .find(|child| !matches!(child.kind(), "line_comment" | "block_comment"))
}

/// Reports whether every operand of the chain is a bare identifier — the
/// remediation asks for names and each operand already carries one.
///
/// The walk descends the logical spine plus the wrappers an operand can wear
/// without ceasing to be a name: parentheses and a unary operator. Every other
/// node kind is an operand that still holds an unnamed expression — a call, a
/// field access, a comparison, a path such as `crate::FLAG`.
fn every_operand_is_named(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "identifier" => true,
        "parenthesized_expression" | "unary_expression" => {
            inner_operand(node).is_some_and(|operand| every_operand_is_named(operand, source))
        }
        "binary_expression" => {
            let (Some(left), Some(right)) = (
                node.child_by_field_name("left"),
                node.child_by_field_name("right"),
            ) else {
                return false;
            };
            is_logical_expression(node, source)
                && every_operand_is_named(left, source)
                && every_operand_is_named(right, source)
        }
        _ => false,
    }
}

/// Reports whether `node` starts its chain — no enclosing `&&` / `||`
/// expression continues it through parentheses or a unary operator.
fn is_chain_root(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if is_logical_expression(parent, source) {
            return false;
        }
        if !TRANSPARENT_WRAPPERS.contains(&parent.kind()) {
            return true;
        }
        current = parent;
    }
    true
}

crate::ast_check! { on ["binary_expression"] prefilter = ["&&", "||"] => |node, source, ctx, diagnostics|
    if !is_logical_expression(node, source) || !is_chain_root(node, source) {
        return;
    }
    let threshold = ctx.config.threshold(super::META.id, "max_ops", ctx.lang);
    if count_logical_ops(node, source) < threshold {
        return;
    }
    if every_operand_is_named(node, source) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        std::sync::Arc::clone(&ctx.path_arc),
        &node,
        super::META.id,
        format!(
            "Expression has {threshold}+ logical operators \u{2014} \
             extract to named variables."
        ),
        Severity::Error,
    ));
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
    fn flags_chain_of_four_operators() {
        let src = "fn f() { let x = a.p() && b || c && d.q() || e; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_three_operators() {
        let src = "fn f() { let x = a.p() && b || c && d.q(); }";
        assert!(run_on(src).is_empty());
    }

    /// Regression for rbaumier/comply#8114 — every operand is already a named
    /// binding, so the remediation is applied and the diagnostic asks for
    /// nothing.
    #[test]
    fn allows_chain_whose_operands_are_all_named() {
        let src = "fn f(a: bool, b: bool, c: bool, d: bool, e: bool) -> bool { if a && b && c && d && e { return true; } false }";
        assert!(run_on(src).is_empty());
    }

    /// Parentheses and `!` leave a name a name.
    #[test]
    fn allows_chain_of_negated_and_parenthesized_names() {
        let src = "fn f() { let ok = !a && (b || c) && !d && e; }";
        assert!(run_on(src).is_empty());
    }

    /// A path is not a bare identifier: `crate::FLAG` names a constant the
    /// reader still has to resolve.
    #[test]
    fn flags_chain_whose_operand_is_a_path() {
        let src = "fn f() { let ok = crate::FLAG && a && b && c && d; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn ignores_comments() {
        let src = "// a && b && c && d && e";
        assert!(run_on(src).is_empty());
    }

    /// Regression for rbaumier/comply#8113 — `&&` inside a string literal is
    /// text, not an operator: the literal carries no `binary_expression`.
    #[test]
    fn ignores_operators_in_string_literal() {
        let src = "fn f() -> &'static str { \"a && b && c && d && e\" }";
        assert!(run_on(src).is_empty());
    }

    /// Regression for rbaumier/comply#8113 — a comment that trails code is
    /// as operator-free as one that opens the line.
    #[test]
    fn ignores_trailing_comment() {
        let src = "fn f() { let x = y; // a && b && c && d && e\n }";
        assert!(run_on(src).is_empty());
    }

    /// Regression for rbaumier/comply#8113 — the chain is the unit, so
    /// wrapping it over several lines does not hide it.
    #[test]
    fn flags_wrapped_operator_chain_once() {
        let src = "fn f() {\n    let ok = a.p()\n        && b.q()\n        && c.r()\n        && d.s()\n        && e.t();\n}";
        assert_eq!(run_on(src).len(), 1);
    }

    /// The chain is the unit, so two chains sharing a line are two findings.
    #[test]
    fn flags_each_chain_of_a_shared_line_separately() {
        let src = "fn f() { let x = a.p() && b.q() && c.r() && d.s() && e.t(); let y = f.p() && g.q() && h.r() && i.s() && j.t(); }";
        assert_eq!(run_on(src).len(), 2);
    }

    /// A macro's arguments are an unparsed token tree — no operator nodes,
    /// so the chain inside one is an accepted false negative.
    #[test]
    fn ignores_operators_in_macro_body() {
        let src = "fn f() { assert!(a.x() && b.y() && c.z() && d.w() && e.v()); }";
        assert!(run_on(src).is_empty());
    }

    /// A let-chain joins its members with anonymous `&&` tokens under
    /// `let_chain`, not with `binary_expression` nodes — an accepted false
    /// negative.
    #[test]
    fn ignores_let_chain() {
        let src = "fn f() { if a && b && let Some(v) = opt && c && d { go(v); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn anchors_on_the_expression_not_the_left_margin() {
        let src = "fn f() {\n    let x = a.p() && b.q() && c.r() && d.s() && e.t();\n}";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert_eq!((diags[0].line, diags[0].column), (2, 13));
    }

    /// A parenthesized sub-chain continues the enclosing chain: one root,
    /// one diagnostic.
    #[test]
    fn counts_a_parenthesized_sub_chain_as_one_expression() {
        let src = "fn f() { let ok = (a.p() && b.q()) && c.r() && d.s() && e.t(); }";
        assert_eq!(run_on(src).len(), 1);
    }

    /// A closure predicate holds its own chain: it neither feeds the
    /// enclosing chain's count nor borrows from it.
    #[test]
    fn closure_chain_is_measured_on_its_own() {
        let src =
            "fn f() { let ok = a.x() && items.iter().any(|i| i.p && i.q && i.r && i.s) && b.y(); }";
        assert!(run_on(src).is_empty());
    }
}
