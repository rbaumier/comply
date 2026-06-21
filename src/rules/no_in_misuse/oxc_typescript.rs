//! no-in-misuse oxc backend — flag `x in arr` where `arr` is actually an array.
//!
//! `in` on an array tests index existence, not membership, so `'k' in arr` is
//! almost always a bug. The rule only fires when the right-hand operand is
//! demonstrably an array: an array literal, an array-typed binding, or a binding
//! initialised from an array-producing expression. A variable's *name* (a `List`
//! suffix, a plural) is never treated as evidence — names do not determine type.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    BinaryOperator, Expression, TSType, TSTypeName, TSTypeOperatorOperator, UnaryOperator,
};
use oxc_span::GetSpan;
use std::sync::Arc;

/// Calls whose result is an array: `[...].map(...)`, `Object.keys(o)`,
/// `Array.from(x)`, `str.split(...)`, etc. Matched on the member/static method
/// name of the callee.
const ARRAY_PRODUCING_METHODS: &[&str] = &[
    "map", "filter", "slice", "splice", "concat", "flat", "flatMap", "split", "sort", "reverse",
    "fill", "from", "of", "keys", "values", "entries",
];

/// Whether a type annotation denotes an array: `T[]`, `readonly T[]`,
/// `Array<T>`, `ReadonlyArray<T>`.
fn type_is_array(ty: &TSType) -> bool {
    match ty {
        TSType::TSArrayType(_) => true,
        TSType::TSTypeOperatorType(op) if op.operator == TSTypeOperatorOperator::Readonly => {
            type_is_array(&op.type_annotation)
        }
        TSType::TSTypeReference(tref) => matches!(
            &tref.type_name,
            TSTypeName::IdentifierReference(id)
                if matches!(id.name.as_str(), "Array" | "ReadonlyArray")
        ),
        _ => false,
    }
}

/// Whether an initializer expression evaluates to an array: an array literal,
/// `new Array(...)`, or an array-producing method/static call.
fn initializer_is_array(expr: &Expression) -> bool {
    match expr {
        Expression::ArrayExpression(_) => true,
        Expression::NewExpression(new_expr) => matches!(
            &new_expr.callee,
            Expression::Identifier(id) if id.name.as_str() == "Array"
        ),
        Expression::CallExpression(call) => callee_produces_array(&call.callee),
        Expression::ParenthesizedExpression(paren) => initializer_is_array(&paren.expression),
        Expression::TSAsExpression(as_expr) => {
            type_is_array(&as_expr.type_annotation) || initializer_is_array(&as_expr.expression)
        }
        Expression::TSSatisfiesExpression(sat) => initializer_is_array(&sat.expression),
        Expression::TSNonNullExpression(nn) => initializer_is_array(&nn.expression),
        _ => false,
    }
}

/// Whether a call's callee is an array-producing method (`x.map`, `Object.keys`,
/// `Array.from`).
fn callee_produces_array(callee: &Expression) -> bool {
    let Expression::StaticMemberExpression(member) = callee else {
        return false;
    };
    ARRAY_PRODUCING_METHODS.contains(&member.property.name.as_str())
}

/// Whether the right-hand operand is actually an array. An array literal is one
/// directly; an identifier is one only if its binding (a `let`/`const`/`var`
/// declarator or a typed parameter) carries an array type annotation or is
/// initialised from an array-producing expression.
fn rhs_is_array<'a>(
    rhs: &Expression,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    match rhs {
        Expression::ArrayExpression(_) => true,
        Expression::Identifier(ident) => binding_is_array(ident, semantic),
        _ => false,
    }
}

/// Resolve an identifier reference to its declaration and decide whether that
/// declaration proves the binding holds an array.
fn binding_is_array<'a>(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let scoping = semantic.scoping();
    let Some(symbol_id) = ident
        .reference_id
        .get()
        .and_then(|ref_id| scoping.get_reference(ref_id).symbol_id())
    else {
        return false;
    };
    let nodes = semantic.nodes();
    let decl_id = scoping.symbol_declaration(symbol_id);
    match nodes.kind(decl_id) {
        AstKind::VariableDeclarator(decl) => {
            if let Some(type_ann) = &decl.type_annotation
                && type_is_array(&type_ann.type_annotation)
            {
                return true;
            }
            decl.init.as_ref().is_some_and(initializer_is_array)
        }
        AstKind::FormalParameter(param) => param
            .type_annotation
            .as_ref()
            .is_some_and(|ann| type_is_array(&ann.type_annotation)),
        _ => false,
    }
}

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

        if !rhs_is_array(&bin.right, semantic) {
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
