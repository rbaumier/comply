//! no-in-misuse oxc backend — flag `x in arr` where `arr` is actually an array.
//!
//! `in` on an array tests index existence, not membership, so `'k' in arr` is
//! almost always a bug. The rule only fires when the right-hand operand is
//! demonstrably an array: an array literal, an array-typed binding, or a binding
//! initialised from an array-producing expression. A variable's *name* (a `List`
//! suffix, a plural) is never treated as evidence — names do not determine type.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, expression_is_array};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression, UnaryOperator};
use oxc_span::GetSpan;
use std::sync::Arc;

/// Span of the topmost expression enclosing the `in` node that still belongs to
/// the same boolean/comparison condition — the boundary inside which a sibling
/// numeric guard can legitimately appear (`typeof K == "number" && K in arr`).
fn enclosing_condition_span<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> oxc_span::Span {
    let mut span = node.kind().span();
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::LogicalExpression(_)
            | AstKind::BinaryExpression(_)
            | AstKind::ParenthesizedExpression(_) => span = ancestor.kind().span(),
            _ => break,
        }
    }
    span
}

/// Whether the in-tested key `lhs_name` is guarded as a number by a sibling
/// `typeof <lhs_name> == "number"` (or `===`) comparison in the enclosing
/// condition. A numeric key makes `K in arr` a sparse-array index-existence
/// check — exactly what `in` is for on arrays — so `.includes()` (a value
/// check) would change semantics and the rule must not fire.
fn key_guarded_as_number<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    lhs_name: &str,
) -> bool {
    let scope = enclosing_condition_span(node, semantic);
    semantic.nodes().iter().any(|n| {
        let AstKind::BinaryExpression(cmp) = n.kind() else {
            return false;
        };
        if !matches!(
            cmp.operator,
            BinaryOperator::Equality | BinaryOperator::StrictEquality
        ) {
            return false;
        }
        if cmp.span.start < scope.start || cmp.span.end > scope.end {
            return false;
        }
        // One side is `typeof <lhs_name>`, the other the string "number".
        let (typeof_arg, number_str) = match (&cmp.left, &cmp.right) {
            (Expression::UnaryExpression(unary), other)
            | (other, Expression::UnaryExpression(unary))
                if unary.operator == UnaryOperator::Typeof =>
            {
                (Some(&unary.argument), is_number_string(other))
            }
            _ => (None, false),
        };
        let Some(Expression::Identifier(id)) = typeof_arg else {
            return false;
        };
        number_str && id.name.as_str() == lhs_name
    })
}

fn is_number_string(expr: &Expression) -> bool {
    matches!(expr, Expression::StringLiteral(lit) if lit.value == "number")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::BinaryExpression(bin) = node.kind() else { return };

        if bin.operator != BinaryOperator::In {
            return;
        }

        // Skip `for ... in` — the parent is a ForInStatement.
        let parent = semantic.nodes().parent_node(node.id());
        if matches!(parent.kind(), AstKind::ForInStatement(_)) {
            return;
        }

        if !expression_is_array(&bin.right, semantic) {
            return;
        }

        // Numeric-index existence check: when the in-tested key `K` is guarded
        // as a number (`typeof K == "number" && K in arr`), `in` correctly
        // tests whether index slot `K` exists, so `.includes()` (a value check)
        // would change semantics.
        if let Expression::Identifier(lhs) = &bin.left
            && key_guarded_as_number(node, semantic, lhs.name.as_str())
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`in` operator checks object keys, not array values — use `.includes()` instead.".into(),
            severity: Severity::Error,
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // --- Genuine arrays still flag ---

    #[test]
    fn flags_in_on_array_literal() {
        assert_eq!(run_on("if (\"x\" in [1, 2, 3]) {}").len(), 1);
    }

    #[test]
    fn flags_in_on_array_initialised_binding() {
        assert_eq!(run_on("const items = [1, 2]; if (\"x\" in items) {}").len(), 1);
    }

    #[test]
    fn flags_in_on_array_typed_binding() {
        assert_eq!(
            run_on("const items: number[] = getThem(); if (1 in items) {}").len(),
            1
        );
    }

    #[test]
    fn flags_in_on_array_generic_typed_binding() {
        assert_eq!(
            run_on("const items: Array<number> = getThem(); if (1 in items) {}").len(),
            1
        );
    }

    #[test]
    fn flags_in_on_readonly_array_typed_binding() {
        assert_eq!(
            run_on("const items: readonly number[] = getThem(); if (1 in items) {}").len(),
            1
        );
    }

    #[test]
    fn flags_in_on_array_typed_parameter() {
        assert_eq!(
            run_on("function f(items: string[]) { return \"x\" in items; }").len(),
            1
        );
    }

    #[test]
    fn flags_in_on_new_array_binding() {
        assert_eq!(
            run_on("const items = new Array(3); if (1 in items) {}").len(),
            1
        );
    }

    #[test]
    fn flags_in_on_map_call_result_binding() {
        assert_eq!(
            run_on("const names = users.map(u => u.name); if (\"x\" in names) {}").len(),
            1
        );
    }

    #[test]
    fn flags_in_on_object_keys_binding() {
        assert_eq!(
            run_on("const keys = Object.keys(obj); if (\"x\" in keys) {}").len(),
            1
        );
    }

    // --- Plain objects / unresolved operands do NOT flag ---

    #[test]
    fn allows_for_in_loop() {
        assert!(run_on("for (const key in obj) {}").is_empty());
    }

    #[test]
    fn allows_in_on_object() {
        assert!(run_on("if (\"name\" in config) {}").is_empty());
    }

    // Regression for #4888: melonjs loader caches. A plain object initialised
    // with `{}` whose name happens to end in `List` is a dictionary, and
    // `key in obj` is the canonical key-existence check — not an array misuse.
    #[test]
    fn allows_in_on_plain_object_named_list() {
        let src = "const binList = {}; if (!(asset.name in binList)) {}";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_in_on_plain_object_cache_return_body() {
        let src = "const tmxList = {}; export function getTMX(elt) { if (elt in tmxList) { return tmxList[elt]; } return null; }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_in_on_object_typed_list_binding() {
        let src = "const imgList: Record<string, HTMLImageElement> = {}; if (\"a\" in imgList) {}";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // A name with an array-ish suffix but no resolvable array binding is not
    // flagged: the name alone is never evidence.
    #[test]
    fn allows_in_on_unresolved_list_name() {
        assert!(run_on(r#"if ("x" in userList) {}"#).is_empty());
    }

    #[test]
    fn allows_in_on_unresolved_items_name() {
        assert!(run_on(r#"if ("x" in items) {}"#).is_empty());
    }

    // Member expressions (`schema.items`) carry no resolvable local-array
    // evidence, so they are never flagged (covers #2100, #3726, #1806).
    #[test]
    fn allows_in_on_member_expression() {
        assert!(run_on(r#"if ("slug" in schema.items) {}"#).is_empty());
    }

    // --- Numeric-index guard on a genuine array still exempts (#3952) ---

    #[test]
    fn allows_in_when_key_guarded_as_number_loose() {
        let src = r#"const elements = [1, 2]; if (typeof key == "number" && key in elements) value = elements[key];"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_in_when_key_guarded_as_number_strict() {
        let src = r#"const arr = [1]; if (typeof i === "number" && i in arr) {}"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Negative space: a `typeof key == "string"` guard is not a numeric-index
    // signal, so `key in arr` on a genuine array still flags.
    #[test]
    fn flags_in_when_guard_is_string_not_number() {
        assert_eq!(
            run_on(r#"const items = [1]; if (typeof key == "string" && key in items) {}"#).len(),
            1
        );
    }

    // Negative space: the numeric guard must be on the SAME identifier as the
    // `in` LHS — a guard on a different variable does not exempt.
    #[test]
    fn flags_in_when_number_guard_is_other_identifier() {
        assert_eq!(
            run_on(r#"const items = [1]; if (typeof other == "number" && key in items) {}"#).len(),
            1
        );
    }
}
