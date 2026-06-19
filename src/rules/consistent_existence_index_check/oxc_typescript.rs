//! consistent-existence-index-check OXC backend — flag `< 0`, `>= 0`, `> -1`
//! on an index-method result. Prefer `=== -1` / `!== -1`.
//!
//! The flagged left operand is either a direct index-method call
//! (`arr.indexOf("x") < 0`) or an identifier whose binding initializer is such
//! a call (`const idx = arr.indexOf("x"); idx < 0`). An identifier merely *named*
//! `*Index`/`*idx` is not enough: an arithmetic index (`i + 1`), a record lookup
//! (`map[key]`), or a parameter holds a plain number, so `< 0` is a real
//! lower-bound check, not an `indexOf` existence check, and rewriting it to
//! `=== -1` would silently change behavior.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression, IdentifierReference, UnaryOperator};
use std::sync::Arc;

pub struct Check;

const INDEX_METHODS: &[&str] = &["indexOf", "lastIndexOf", "findIndex", "findLastIndex"];

fn is_index_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else { return false };
    let Expression::StaticMemberExpression(member) = &call.callee else { return false };
    INDEX_METHODS.contains(&member.property.name.as_str())
}

fn is_index_identifier(expr: &Expression, semantic: &oxc_semantic::Semantic) -> bool {
    let Expression::Identifier(id) = expr else { return false };
    let lower = id.name.as_str().to_ascii_lowercase();
    if !(lower.contains("index") || lower.contains("idx")) {
        return false;
    }
    binding_init_is_index_call(id, semantic)
}

/// True when `id` resolves to a variable whose initializer is an index-method
/// call (`const idx = arr.indexOf("x")`). An arithmetic index (`i + 1`), a
/// record lookup (`m[key]`), a parameter, or any non-index-method initializer
/// is a plain numeric value, so `< 0` is a real lower-bound check, not an
/// `indexOf`-existence check.
///
/// Resolves the binding via `reference_id` → symbol → declaration node, then
/// inspects the enclosing `VariableDeclarator`'s `init`, mirroring
/// [`crate::oxc_helpers::is_local_object_builder_binding`].
fn binding_init_is_index_call(id: &IdentifierReference, semantic: &oxc_semantic::Semantic) -> bool {
    let Some(ref_id) = id.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        if let AstKind::VariableDeclarator(decl) = kind {
            return decl.init.as_ref().is_some_and(is_index_call);
        }
    }
    false
}

fn is_index_expr(expr: &Expression, semantic: &oxc_semantic::Semantic) -> bool {
    is_index_call(expr) || is_index_identifier(expr, semantic)
}

fn is_zero(expr: &Expression) -> bool {
    matches!(expr, Expression::NumericLiteral(n) if n.value == 0.0)
}

fn is_negative_one(expr: &Expression) -> bool {
    if let Expression::UnaryExpression(u) = expr
        && u.operator == UnaryOperator::UnaryNegation
            && let Expression::NumericLiteral(n) = &u.argument {
                return n.value == 1.0;
            }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["indexOf", "lastIndexOf", "findIndex", "findLastIndex"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::BinaryExpression(bin) = node.kind() else { return };

        let op = bin.operator;
        let is_bad = if (op == BinaryOperator::LessThan || op == BinaryOperator::GreaterEqualThan)
            && is_zero(&bin.right)
        {
            is_index_expr(&bin.left, semantic)
        } else if op == BinaryOperator::GreaterThan && is_negative_one(&bin.right) {
            is_index_expr(&bin.left, semantic)
        } else {
            false
        };

        if !is_bad {
            return;
        }

        let message = if op == BinaryOperator::LessThan {
            "Prefer `=== -1` over `< 0` to check index non-existence."
        } else {
            "Prefer `!== -1` over `>= 0` / `> -1` to check index existence."
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: message.into(),
            severity: Severity::Warning,
            span: None,
        });
    }
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    // ── True positives preserved ──────────────────────────────────────────

    // An identifier whose binding is an `indexOf` result is still flagged: the
    // value really is `-1` on absence, so `< 0` should be `=== -1`.
    #[test]
    fn flags_indexof_binding_less_than_zero() {
        let src = r#"const idx = arr.indexOf("x"); if (idx < 0) {}"#;
        assert_eq!(run(src).len(), 1);
    }

    // `>= 0` on an index-method binding is the existence check — still flagged.
    #[test]
    fn flags_indexof_binding_greater_equal_zero() {
        let src = r#"const idx = arr.indexOf("x"); if (idx >= 0) {}"#;
        assert_eq!(run(src).len(), 1);
    }

    // `> -1` on a `findIndex` binding is the existence check — still flagged.
    #[test]
    fn flags_findindex_binding_greater_than_negative_one() {
        let src = r#"const itemIdx = arr.findIndex(p); if (itemIdx > -1) {}"#;
        assert_eq!(run(src).len(), 1);
    }

    // A direct index-method call is unaffected by the provenance change.
    #[test]
    fn flags_direct_indexof_call() {
        let src = r#"if (arr.indexOf("x") < 0) {}"#;
        assert_eq!(run(src).len(), 1);
    }

    // ── Regression #3727: a `*Index`/`*idx` name without index-method
    //    provenance is a plain number, so `< 0` is a real lower-bound check ──

    // Arithmetic: `index + add` can be negative (e.g. -3); `< 0` ≠ `=== -1`.
    #[test]
    fn ignores_arithmetic_index() {
        let src = r#"const found = arr.indexOf("x"); const nextIndex = index + add; if (nextIndex < 0) {}"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Ternary arithmetic: bounds value, not an index-method result.
    #[test]
    fn ignores_ternary_arithmetic_index() {
        let src = r#"const found = arr.indexOf("x"); const newIndex = goForward ? a : b; if (newIndex < 0) {}"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Record lookup: `keyToIndex[e.key]` is a plain number with wrap-around.
    #[test]
    fn ignores_record_lookup_index() {
        let src = r#"const found = arr.indexOf("x"); const itemIndex = keyToIndex[e.key]; if (itemIndex < 0) {}"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // A parameter named `index` has no index-method binding.
    #[test]
    fn ignores_parameter_named_index() {
        let src = r#"const found = arr.indexOf("x"); function f(index) { if (index < 0) {} }"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }
}
