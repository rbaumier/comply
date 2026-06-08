//! ts-no-as-narrowing OxcCheck backend — forbid `as` used to narrow types.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, name_is_generic_type_param_in_scope};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, TSType, TSTypeName};
use oxc_span::GetSpan;

pub struct Check;

const NARROWING_UTILITY_TYPES: &[&str] = &[
    "NonNullable",
    "Exclude",
    "Extract",
    "Required",
    "Readonly",
    "Pick",
    "Capitalize",
    "Uncapitalize",
    "Uppercase",
    "Lowercase",
];

fn target_is_narrowing(ty: &TSType, _source: &str) -> bool {
    match ty {
        TSType::TSLiteralType(_) | TSType::TSTemplateLiteralType(_) => true,
        TSType::TSTypeReference(r) => {
            let TSTypeName::IdentifierReference(id) = &r.type_name else { return false };
            let name = id.name.as_str();
            if r.type_arguments.is_some() {
                // Generic utility type like `NonNullable<T>`.
                NARROWING_UTILITY_TYPES.contains(&name)
            } else {
                // PascalCase identifier — likely a user-defined narrowing type.
                name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            }
        }
        _ => false,
    }
}

fn peel_parens<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
    let mut current = expr;
    while let Expression::ParenthesizedExpression(p) = current {
        current = &p.expression;
    }
    current
}

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
        let AstKind::TSAsExpression(as_expr) = node.kind() else {
            return;
        };

        // Tests cast runtime values after a runtime guard
        // (`expect(x).toBeInstanceOf(Foo); (x as Foo).field`) — the assertion is
        // backed by the guard, not standing in for narrowing. Skip test files.
        if ctx.file.path_segments.in_test_dir {
            return;
        }

        // Skip `as const`.
        let type_text = &ctx.source
            [as_expr.type_annotation.span().start as usize..as_expr.type_annotation.span().end as usize];
        if type_text.trim() == "const" {
            return;
        }

        if !target_is_narrowing(&as_expr.type_annotation, ctx.source) {
            return;
        }

        // Skip `as TParam` when `TParam` is a generic type parameter on an
        // enclosing function/method/class/interface/type alias. These are
        // structural type-bridge casts (e.g. TanStack Router's
        // `useSearch() as TSearch`), not narrowings.
        if let TSType::TSTypeReference(r) = &as_expr.type_annotation
            && r.type_arguments.is_none()
        {
            let TSTypeName::IdentifierReference(id) = &r.type_name else { return };
            let name = id.name.as_str();
            if name_is_generic_type_param_in_scope(name, node.id(), semantic) {
                return;
            }
        }

        // Skip the outer half of `x as unknown as T` — the canonical
        // contravariant-boundary escape hatch (e.g. Drizzle relational types
        // invariant in `TablesRelationalConfig`). The inner cast must be to
        // the `unknown` keyword specifically; `x as Foo as Bar` is NOT
        // exempted. Peel any parentheses wrapping the inner expression so
        // that `(x as unknown) as T` is treated identically to
        // `x as unknown as T`.
        if let Expression::TSAsExpression(inner) = peel_parens(&as_expr.expression)
            && matches!(inner.type_annotation, TSType::TSUnknownKeyword(_))
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, as_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Avoid using `as` to narrow types; use a type predicate or `in`/`typeof` check.".into(),
            severity: Severity::Warning,
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
    fn flags_pascal_user_type() {
        assert_eq!(run_on("const x = value as AdminUser;").len(), 1);
    }

    #[test]
    fn allows_guarded_cast_in_test_files() {
        // Regression for issue #573: assertion after a runtime `instanceof` guard.
        use crate::rules::file_ctx::{FileCtx, PathSegments};
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..Default::default() },
            ..Default::default()
        };
        assert!(
            crate::rules::test_helpers::run_oxc_tsx_with_file_ctx(
                "const c = (apiError as InternalError).cause;",
                &Check,
                &file,
            )
            .is_empty()
        );
    }

    #[test]
    fn flags_literal_target() {
        assert_eq!(run_on("const x = val as 'foo';").len(), 1);
    }

    #[test]
    fn allows_generic_type_param_in_function() {
        // Regression for #114: `as TSearch` where `<TSearch>` is on the
        // enclosing function is a structural type bridge, not a narrowing.
        let src = "function useTypedSearch<TSearch>(api: { useSearch: () => unknown }) {\n\
                   const search = api.useSearch() as TSearch;\n\
                   return search;\n}";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_generic_type_param_in_class_method() {
        let src = "class Wrap<T> { unwrap(v: unknown) { return v as T; } }";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn flags_pascal_cased_when_not_generic_param() {
        let diags = run_on("function f() { return x as MyType; }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_outer_cast_of_as_unknown_as_chain_drizzle_repro() {
        // Regression for #178: Drizzle's relational types are invariant in
        // `TablesRelationalConfig`, so a structural relabel of a deeply-
        // generic filter requires `as unknown as <Type>`. The outer half
        // must not be flagged as a narrowing.
        let src = "type AnyRelationsFilter = unknown;\n\
                   declare const where: object;\n\
                   const widenedWhere = where as unknown as AnyRelationsFilter;";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_simple_as_unknown_as_t() {
        let diags = run_on("const y = x as unknown as Foo;");
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn flags_single_cast_to_pascal_type() {
        // Negative: a plain `x as Foo` is still a narrowing.
        let diags = run_on("const y = x as Foo;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_double_cast_without_unknown_middle() {
        // Negative: `x as any as Foo` is NOT the canonical escape hatch —
        // the middle must be `unknown` for the exemption to apply. The
        // outer cast (target `Foo`) must still flag as a narrowing.
        let diags = run_on("const y = x as any as Foo;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_parenthesised_unknown_in_double_cast() {
        // Issue #178 follow-up — `(x as unknown) as Foo` is semantically
        // identical to `x as unknown as Foo`.
        let src = "declare const x: unknown; const y = (x as unknown) as Foo;";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn still_flags_triple_parenthesised_as_chain() {
        // `((x as A) as unknown) as B` — the middle isn't a plain `as unknown`
        // of the original value; the inner `as A` is the suspect cast. We
        // don't auto-exempt arbitrary triple casts.
        let src = "declare const x: unknown; const y = ((x as A) as unknown) as B;";
        let diags = run_on(src);
        assert!(!diags.is_empty(), "expected at least one diag for inner `as A` cast");
    }



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_cast_to_string_literal() {
        let diags = run("const x = val as 'foo';");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn flags_cast_to_number_literal() {
        let diags = run("const x = val as 42;");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_as_const() {
        assert!(run("const x = [1, 2] as const;").is_empty());
    }


    #[test]
    fn allows_cast_to_regular_type() {
        assert!(run("const x = val as string;").is_empty());
    }


    #[test]
    fn flags_cast_to_pascal_case_type() {
        assert_eq!(run("const x = value as AdminUser;").len(), 1);
    }


    #[test]
    fn flags_cast_to_non_nullable() {
        assert_eq!(run("const x = value as NonNullable<T>;").len(), 1);
    }


    #[test]
    fn flags_cast_to_exclude() {
        assert_eq!(run("const x = value as Exclude<T, null>;").len(), 1);
    }


    #[test]
    fn allows_cast_to_any() {
        assert!(run("const x = value as any;").is_empty());
    }


    #[test]
    fn allows_cast_to_unknown() {
        assert!(run("const x = value as unknown;").is_empty());
    }


    #[test]
    fn allows_cast_to_lowercase_alias() {
        assert!(run("const x = value as myAlias;").is_empty());
    }


    #[test]
    fn still_flags_non_framework_pascal_case() {
        // Regular user-defined PascalCase types must still be flagged
        assert_eq!(run("const x = value as AdminUser;").len(), 1);
    }
}
