//! better-auth-no-duplicate-baseurl oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["BETTER_AUTH_URL"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be `betterAuth`.
        let Expression::Identifier(ident) = &call.callee else {
            return;
        };
        if ident.name.as_str() != "betterAuth" {
            return;
        }

        // Only flag when the file references BETTER_AUTH_URL.
        if !ctx.source.contains("BETTER_AUTH_URL") {
            return;
        }

        // Find the first object argument.
        let Some(obj_arg) = call.arguments.iter().find_map(|arg| {
            if let Argument::ObjectExpression(obj) = arg {
                Some(obj)
            } else {
                None
            }
        }) else {
            return;
        };

        // Find a property with key `baseURL`.
        let Some(base_url_prop) = obj_arg.properties.iter().find_map(|p| {
            if let ObjectPropertyKind::ObjectProperty(prop) = p {
                let key_name = match &prop.key {
                    PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
                    PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
                    _ => None,
                };
                if key_name == Some("baseURL") {
                    return Some(prop);
                }
            }
            None
        }) else {
            return;
        };

        let (line, column) =
            byte_offset_to_line_col(ctx.source, base_url_prop.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`baseURL` duplicates `BETTER_AUTH_URL` — remove it and use the env var."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
