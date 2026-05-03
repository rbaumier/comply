//! ts-no-as-narrowing OxcCheck backend — forbid `as` used to narrow types.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::TSType;
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

fn target_is_narrowing(ty: &TSType, source: &str) -> bool {
    match ty {
        TSType::TSLiteralType(_) | TSType::TSTemplateLiteralType(_) => true,
        TSType::TSTypeReference(r) => {
            let name = &source[r.type_name.span().start as usize..r.type_name.span().end as usize];
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
        _semantic: &'a oxc_semantic::Semantic<'a>,
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
