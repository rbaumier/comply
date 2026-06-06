//! zod-prefer-stringbool oxc backend — flag `z.coerce.boolean()` in form contexts.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const FORM_INDICATORS: &[&str] = &[
    "react-hook-form",
    "@tanstack/react-form",
    "@tanstack/form",
    "formik",
    "FormData",
    "URLSearchParams",
    "useForm",
    "searchParams",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["z.coerce.boolean"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be a member expression `z.coerce.boolean`
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "boolean" {
            return;
        }

        // The object should be `z.coerce`
        let Expression::StaticMemberExpression(inner) = &member.object else { return };
        if inner.property.name.as_str() != "coerce" {
            return;
        }
        let Expression::Identifier(z_ident) = &inner.object else { return };
        if z_ident.name.as_str() != "z" {
            return;
        }

        // Only flag in form contexts
        if !FORM_INDICATORS.iter().any(|m| ctx.source_contains(m)) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`z.coerce.boolean()` treats every non-empty string as `true` — \
                      use `z.stringbool()` for HTML form inputs and query strings."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
