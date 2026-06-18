//! ts-no-as-narrowing OxcCheck backend — forbid `as` used to narrow types.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{
    byte_offset_to_line_col, is_inside_type_predicate_fn, is_outer_as_unknown_double_cast,
    name_is_generic_type_param_in_scope,
};
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

/// Built-in DOM element interfaces (`HTMLSpanElement`, `SVGPathElement`, the
/// bare `HTMLElement`/`Element`, …). DOM query APIs return the broad
/// `HTMLElement | null` / `Element | null`, so casting their result to a
/// specific element interface is the idiomatic way to access element-specific
/// members — `instanceof` narrowing is equivalent but verbose, not a smell.
fn is_dom_element_type(name: &str) -> bool {
    name == "Element"
        || ((name.starts_with("HTML") || name.starts_with("SVG") || name.starts_with("MathML"))
            && name.ends_with("Element"))
}

/// Whether the cast operand is a freshly-constructed literal value: an object
/// literal (`{} as RouteModules`), an array literal (`[1, 2] as ReadonlyArray<number>`),
/// or a primitive literal (`"idle" as RevalidationState`). Casting such an
/// operand is a construction-time type ascription, not a narrowing of a
/// pre-existing binding — there is no variable to refine with a type predicate
/// or `in`/`typeof` check, so the rule's remediation does not apply.
fn operand_is_constructed_literal(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::ObjectExpression(_)
            | Expression::ArrayExpression(_)
            | Expression::StringLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
            | Expression::TemplateLiteral(_)
            | Expression::BigIntLiteral(_)
            | Expression::RegExpLiteral(_)
    )
}

fn target_is_narrowing(ty: &TSType, _source: &str) -> bool {
    match ty {
        TSType::TSLiteralType(_) | TSType::TSTemplateLiteralType(_) => true,
        TSType::TSTypeReference(r) => {
            let TSTypeName::IdentifierReference(id) = &r.type_name else { return false };
            let name = id.name.as_str();
            if r.type_arguments.is_some() {
                // Generic utility type like `NonNullable<T>`.
                NARROWING_UTILITY_TYPES.contains(&name)
            } else if is_dom_element_type(name) {
                false
            } else {
                // PascalCase identifier — likely a user-defined narrowing type.
                name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            }
        }
        _ => false,
    }
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

        // Skip type-ascriptions of a freshly-constructed literal operand
        // (`{} as T`, `[1, 2] as T`, `"idle" as Union`). These ascribe a type
        // to a value being built inline; there is no pre-existing binding to
        // narrow, so a type predicate / `in` / `typeof` guard cannot apply.
        if operand_is_constructed_literal(&as_expr.expression) {
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
        // exempted. This rule exempts ONLY the outer half (not the inner
        // `as unknown`); parentheses are peeled so `(x as unknown) as T` is
        // treated identically to `x as unknown as T`.
        if is_outer_as_unknown_double_cast(as_expr) {
            return;
        }

        // Skip `as` casts inside the body of a type-predicate function
        // (`value is T`). That function IS the custom type guard this rule
        // recommends; the cast is needed to read properties off the
        // loosely-typed input, so flagging it would be circular advice.
        if is_inside_type_predicate_fn(node.id(), semantic) {
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
            crate::rules::test_helpers::run_rule_with_ctx(&Check, "const c = (apiError as InternalError).cause;", "t.tsx", crate::project::default_static_project_ctx(), &file)
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
    fn allows_dom_element_subtype_cast() {
        // Regression for #1080: DOM query APIs return the broad
        // `HTMLElement | null`, so casting to a specific element interface is
        // the idiomatic refinement, not a narrowing smell.
        let src = "let secretDisplay: HTMLSpanElement = document.getElementById(\"secret-display\") as HTMLSpanElement;\n\
                   let secretButton: HTMLButtonElement = document.getElementById(\"secret-button\") as HTMLButtonElement;\n\
                   const el = document.querySelector(\".foo\") as HTMLInputElement;\n\
                   const svg = node as SVGPathElement;\n\
                   const generic = root as Element;";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn still_flags_non_dom_pascal_type() {
        // Control: a user-defined PascalCase target is still a narrowing,
        // even though it superficially resembles a DOM type name.
        let diags = run_on("const x = value as AdminElement;");
        assert_eq!(diags.len(), 1);
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

    #[test]
    fn allows_as_in_arrow_type_predicate_body() {
        // Regression for #1976: an arrow whose return type is a type predicate
        // (`api is WithDispatch`) IS the custom type guard this rule
        // recommends; the `as` casts in its body read properties off the
        // `unknown` input and must not be flagged.
        let src = "const shouldDispatchFromDevtools = (api: unknown): api is WithDispatch =>\n\
                   !!(api as WithDispatch).dispatchFromDevtools &&\n\
                   typeof (api as WithDispatch).dispatch === 'function';";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_as_in_function_type_predicate_body() {
        // Regression for #1976: a `function isFoo(x): x is Foo` guard body.
        let src = "function isFoo(x: unknown): x is Foo { return (x as Foo).bar !== undefined; }";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn still_flags_as_in_non_predicate_function() {
        // Control for #1976: the same cast in a function WITHOUT a
        // type-predicate return type is still a narrowing and must fire.
        let src = "function f(x: unknown) { return (x as Foo).bar; }";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1, "expected one diag: {:?}", diags);
    }

    #[test]
    fn allows_object_literal_ascription() {
        // Regression for #3875: `{} as T` seeds a typed accumulator; the
        // object literal is constructed inline, so there is no binding to
        // narrow with a type predicate.
        let diags = run_on("const seed = {} as RouteModules;");
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_object_spread_literal_ascription() {
        // Regression for #3875: a spread object literal is still a freshly
        // constructed value, not a pre-existing binding.
        let diags = run_on("const l = { ...link, rel: \"prefetch\", as: \"style\" } as Link;");
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_string_literal_ascription() {
        // Regression for #3875: `"idle" as Union` ascribes a union member to a
        // primitive literal; there is no variable to refine.
        let diags = run_on("const r = \"idle\" as RevalidationState;");
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_array_literal_ascription() {
        // Regression for #3875: `[1, 2] as ReadonlyArray<number>` ascribes a
        // type to a freshly constructed array literal.
        let diags = run_on("const a = [1, 2] as ReadonlyArray<number>;");
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn still_flags_identifier_operand_narrowing() {
        // Control for #3875: an Identifier operand IS a pre-existing binding;
        // `existingVar as NarrowType` is a genuine narrowing and must fire.
        let diags = run_on("const x = existingVar as NarrowType;");
        assert_eq!(diags.len(), 1, "expected one diag: {:?}", diags);
    }

    #[test]
    fn still_flags_member_expression_operand_narrowing() {
        // Control for #3875: a member-expression operand reads a pre-existing
        // value; `foo.bar as T` is still a narrowing.
        let diags = run_on("const x = foo.bar as Narrowed;");
        assert_eq!(diags.len(), 1, "expected one diag: {:?}", diags);
    }
}
