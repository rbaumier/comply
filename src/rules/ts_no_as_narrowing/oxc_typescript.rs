//! ts-no-as-narrowing OxcCheck backend — forbid `as` used to narrow types.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, name_is_generic_type_param_in_scope};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{TSType, TSTypeName};
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
}
