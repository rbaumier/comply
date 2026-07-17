//! jsx-no-leaked-render oxc backend — flag `{expr && <JSX/>}` only when `expr`
//! can leak a *visible* falsy value into the DOM.
//!
//! `{expr && <JSX/>}` renders a visible leak only when `expr` is `number`,
//! `string`, or `bigint`: their falsy values (`0`, `""`, `0n`) render as text.
//! Every other type's falsy value is `null`/`undefined`/`false`, which React
//! renders as nothing — so an object, a component, a `ReactNode`, or a
//! `FieldError | undefined` operand can never leak. The rule therefore fires
//! only when the left operand is *provably* number/string/bigint: a
//! `.length`/`.size` member (always numeric), a literal, or an identifier whose
//! binding resolves to such a type or literal initializer. A variable's *name*
//! is never evidence — an operand whose type cannot be proven number/string is
//! not flagged (the broad "any `&&`-JSX" stance belongs to the sibling rule
//! `react-no-and-conditional-jsx`).

use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, LogicalOperator, TSType};
use std::sync::Arc;

pub struct Check;

/// True if the expression is a JSX element/fragment.
fn is_jsx(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::JSXElement(_) | Expression::JSXFragment(_)
    )
}

/// Whether a TS type can hold a value that renders as a *visible* falsy leak.
/// Only `number`/`string`/`bigint` produce a visible `0`/`""`/`0n` through
/// `{expr && <JSX/>}`; every other type's falsy value is `null`/`undefined`/
/// `false`, which React renders as nothing. A union leaks if any member is one
/// of these keywords (`string | undefined` still leaks via `""`).
fn type_can_leak(ty: &TSType) -> bool {
    match ty {
        TSType::TSNumberKeyword(_) | TSType::TSStringKeyword(_) | TSType::TSBigIntKeyword(_) => true,
        TSType::TSUnionType(union) => union.types.iter().any(type_can_leak),
        TSType::TSParenthesizedType(paren) => type_can_leak(&paren.type_annotation),
        _ => false,
    }
}

/// Whether an initializer expression is a bare numeric/string/bigint literal —
/// the only initializers that prove a binding is number/string/bigint without a
/// type annotation.
fn initializer_can_leak(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::NumericLiteral(_) | Expression::StringLiteral(_) | Expression::BigIntLiteral(_)
    )
}

/// Resolve an identifier reference to its declaration and decide whether that
/// declaration proves the binding is number/string/bigint — a `let`/`const`/`var`
/// declarator carrying such a type annotation or initialised from a
/// numeric/string/bigint literal, or a parameter typed as one. An unresolved,
/// imported, or otherwise unproven binding returns `false`.
fn binding_can_leak(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
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
                && type_can_leak(&type_ann.type_annotation)
            {
                return true;
            }
            decl.init.as_ref().is_some_and(initializer_can_leak)
        }
        AstKind::FormalParameter(param) => param
            .type_annotation
            .as_ref()
            .is_some_and(|ann| type_can_leak(&ann.type_annotation)),
        _ => false,
    }
}

/// Whether the left operand of `{operand && <JSX/>}` can leak a *visible* falsy
/// value (`0`/`""`/`0n`) into the DOM — the only case this rule flags. True only
/// when the operand is provably number/string/bigint: a `.length`/`.size` member
/// (always numeric), a literal, or an identifier whose binding resolves to such a
/// type or literal initializer. Anything unproven (objects, components,
/// `FieldError`, `ReactNode`, unresolved/imported bindings) returns `false`.
fn operand_can_leak(expr: &Expression, semantic: &oxc_semantic::Semantic) -> bool {
    match expr {
        Expression::StaticMemberExpression(member) => {
            matches!(member.property.name.as_str(), "length" | "size")
        }
        Expression::Identifier(ident) => binding_can_leak(ident, semantic),
        Expression::NumericLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::BigIntLiteral(_) => true,
        Expression::ParenthesizedExpression(paren) => operand_can_leak(&paren.expression, semantic),
        Expression::TSAsExpression(as_expr) => {
            type_can_leak(&as_expr.type_annotation)
                || operand_can_leak(&as_expr.expression, semantic)
        }
        Expression::TSSatisfiesExpression(sat) => operand_can_leak(&sat.expression, semantic),
        Expression::TSNonNullExpression(nn) => operand_can_leak(&nn.expression, semantic),
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXExpressionContainer]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXExpressionContainer(container) = node.kind() else {
            return;
        };
        let oxc_ast::ast::JSXExpression::LogicalExpression(logical) = &container.expression else {
            return;
        };
        if logical.operator != LogicalOperator::And {
            return;
        }
        // Right side must contain JSX.
        if !is_jsx(&logical.right) {
            return;
        }
        // Flag only when the left operand is provably number/string/bigint — the
        // sole types whose falsy value (`0`/`""`/`0n`) renders as a visible leak.
        if !operand_can_leak(&logical.left, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, logical.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Potential leaked render — numeric/string value with `&&` renders \
                      falsy value (`0`, `\"\"`) instead of nothing."
                .into(),
            severity: super::META.severity,
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    // --- Provable number/string/bigint operands still flag ---

    #[test]
    fn flags_length_member() {
        let src = "const x = <div>{items.length && <List />}</div>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_size_member() {
        let src = "const x = <div>{selected.size && <List />}</div>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_string_typed_binding() {
        // A `string`-typed binding can be `""` — a real (if invisible) leak the
        // rule still owns.
        let src = "const f = (title: string) => <div>{title && <h1>{title}</h1>}</div>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_number_typed_binding() {
        let src = "const f = (count: number) => <div>{count && <Component />}</div>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_numeric_literal_initialised_binding() {
        let src = "const n = 5; const x = <div>{n && <Component />}</div>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_string_union_typed_binding() {
        // `string | undefined` still leaks via `""`.
        let src = "const f = (label: string | undefined) => <div>{label && <span>{label}</span>}</div>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_parenthesized_number_binding() {
        let src = "const f = (count: number) => <div>{(count) && <Component />}</div>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_as_number_cast() {
        let src = "const x = <div>{(value as number) && <Component />}</div>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_bigint_literal() {
        let src = "const x = <div>{0n && <Component />}</div>;";
        assert_eq!(run_on(src).len(), 1);
    }

    // --- Boolean-producing guards do not flag ---

    #[test]
    fn allows_single_bang() {
        let src = "const a = <div>{!isCloud && <SecurityTip />}</div>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_single_bang_on_optional_length() {
        let src = "const b = <div>{!activeSurveys?.length && <p>-</p>}</div>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_double_bang() {
        let src = "const c = <div>{!!count && <X />}</div>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_comparison() {
        let src = "const x = <div>{count > 0 && <Component />}</div>;";
        assert!(run_on(src).is_empty());
    }

    // --- Unproven operands are not flagged (safe default) ---

    // An unresolved identifier carries no type evidence, so it is not flagged —
    // the name alone (`isReady`, `count`, …) is never evidence.
    #[test]
    fn allows_unresolved_identifier() {
        let src = "const x = <div>{someFlag && <Component />}</div>;";
        assert!(run_on(src).is_empty());
    }

    // Regression for #7653: an object|null|undefined binding cannot leak (its
    // falsy values are null/undefined, which render as nothing).
    #[test]
    fn allows_object_typed_binding() {
        let src = "const f = (createdByDetails: Details | null | undefined) => <div>{createdByDetails && <ButtonAvatars showTooltip={false} userIds={createdByDetails?.id} />}</div>;";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Regression for #7653: a component operand guarded before render — the outer
    // `&&` left is a nested logical expression, never a number/string.
    #[test]
    fn allows_component_guard() {
        let src = "const x = <div>{!hideIcon && Icon && <Icon className=\"h-3 w-3\" />}</div>;";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Regression for #7653: react-hook-form `errors.first_name` is a
    // `FieldError | undefined` — a non-`length`/`size` member is not flagged.
    #[test]
    fn allows_field_error_member() {
        let src = "const x = <div>{errors.first_name && <span>{errors.first_name.message}</span>}</div>;";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Regression for #7653: a `ReactNode`-typed operand renders nothing when
    // falsy.
    #[test]
    fn allows_reactnode_typed_binding() {
        let src = "const f = (control: React.ReactNode) => <div>{control && <div>{control}</div>}</div>;";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // A member read that is neither `.length` nor `.size` carries no numeric
    // evidence (e.g. a Vue ref `.value` holding an object), so it is not flagged.
    #[test]
    fn allows_non_length_member() {
        let src = "const t = () => <div>{showText.value && <span>hi</span>}</div>;";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }
}
