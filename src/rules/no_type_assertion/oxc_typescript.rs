//! no-type-assertion OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, name_is_generic_type_param_in_scope};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, TSArrayType, TSType, TSTypeName};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSAsExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSAsExpression(as_expr) = node.kind() else { return };

        // Allow `as const` — it's a type refinement, not a cast.
        let type_span = as_expr.type_annotation.span();
        let type_text = &ctx.source[type_span.start as usize..type_span.end as usize];
        if type_text == "const" {
            return;
        }

        // Allow `as never` and `as never[]` — explicit type-system escape
        // hatches used to bridge generic-table erasure (e.g. Drizzle's
        // `tx.insert(table).values(values as never[])`). These are not
        // narrowings; they're acknowledgements that the call site can't be
        // typed precisely against a generic library helper.
        if matches!(as_expr.type_annotation, TSType::TSNeverKeyword(_)) {
            return;
        }
        if let TSType::TSArrayType(arr) = &as_expr.type_annotation
            && let TSArrayType { element_type, .. } = &**arr
            && matches!(element_type, TSType::TSNeverKeyword(_))
        {
            return;
        }

        // Allow `as TParam` when `TParam` is a generic type parameter declared
        // on the enclosing function/method/class/interface/type-alias. These
        // are structural type-bridge casts (TanStack Router, generic wrappers,
        // etc.) — not narrowings, not escape hatches.
        if let TSType::TSTypeReference(r) = &as_expr.type_annotation
            && r.type_arguments.is_none()
        {
            let TSTypeName::IdentifierReference(id) = &r.type_name else { return };
            let name = id.name.as_str();
            if name_is_generic_type_param_in_scope(name, node.id(), semantic) {
                return;
            }
        }

        // Allow either half of an `x as unknown as T` chain — the chain is
        // the canonical contravariant-boundary escape hatch (matches the
        // `no-double-cast` skip). Without these two checks, the outer cast
        // and the inner `as unknown` still fire even though `no-double-cast`
        // correctly stays silent.
        //  - Outer half: `x as unknown as T` whose inner is `_ as unknown`.
        if let Expression::TSAsExpression(inner) = &as_expr.expression
            && matches!(inner.type_annotation, TSType::TSUnknownKeyword(_))
        {
            return;
        }
        //  - Inner half: `_ as unknown` whose parent is another TSAsExpression.
        if matches!(as_expr.type_annotation, TSType::TSUnknownKeyword(_))
            && matches!(
                semantic.nodes().parent_node(node.id()).kind(),
                AstKind::TSAsExpression(_)
            )
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, as_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Type assertion `as T` bypasses the type checker — use `satisfies`, type guards, or generics.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_as_string() {
        let diags = run_on("const x = foo as string;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_as_const() {
        assert!(run_on("const x = { a: 1 } as const;").is_empty());
    }

    #[test]
    fn allows_generic_type_param_in_function() {
        // Regression for #114: `as TSearch` where `<TSearch>` is on the
        // enclosing function is a structural type bridge, not a cast.
        let src = "function useTypedSearch<TSearch>(api: { useSearch: () => unknown }) {\n\
                   const search = api.useSearch() as TSearch;\n\
                   return search;\n}";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_generic_type_param_in_arrow() {
        let src = "const f = <T>(x: unknown) => x as T;";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn flags_pascal_cased_when_not_generic_param() {
        // PascalCase looks like a generic param but isn't declared in scope.
        let diags = run_on("function f() { return x as MyType; }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_as_never_array_for_drizzle_generic_table() {
        // Regression for #127: `as never[]` bridges generic-table erasure
        // when calling a generic Drizzle helper from a generic wrapper —
        // an explicit type-system escape hatch, not a narrowing.
        let src = "async function replaceRows<TRow>(tx: any, table: any, values: readonly TRow[]) {\n\
                   tx.insert(table).values(values as never[]);\n}";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_as_never() {
        // `as never` is the canonical "this branch is unreachable" /
        // generic-bridge escape hatch.
        let diags = run_on("const x = foo as never;");
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_outer_cast_of_as_unknown_as_chain() {
        // Regression for #114: the outer cast of `x as unknown as T` is part
        // of the canonical contravariant-boundary escape hatch and should
        // stay silent (mirrors the `no-double-cast` skip).
        let src = "const navigate = api.useNavigate() as unknown as \
                   (options: { search: (p: TSearch) => TSearch }) => unknown;";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_drizzle_relations_filter_as_unknown_as_chain() {
        // Regression for #178: Drizzle relational types are invariant in
        // `TablesRelationalConfig`; structural relabel requires
        // `as unknown as <Type>`.
        let src = "type AnyRelationsFilter = unknown;\n\
                   declare const where: object;\n\
                   const widenedWhere = where as unknown as AnyRelationsFilter;";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn flags_double_cast_without_unknown_middle() {
        // Negative: `x as any as Foo` is NOT the canonical escape hatch —
        // the middle must be `unknown` for the exemption to apply.
        let diags = run_on("const y = x as any as Foo;");
        assert!(!diags.is_empty(), "expected at least one diag");
    }
}
