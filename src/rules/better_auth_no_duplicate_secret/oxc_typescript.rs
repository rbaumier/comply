//! better-auth-no-duplicate-secret — oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["BETTER_AUTH_SECRET"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be `betterAuth`.
        let Expression::Identifier(ident) = &call.callee else { return };
        if ident.name.as_str() != "betterAuth" {
            return;
        }

        // Only flag when the file actually references BETTER_AUTH_SECRET.
        if !ctx.source.contains("BETTER_AUTH_SECRET") {
            return;
        }

        // First argument must be an object literal.
        let Some(first_arg) = call.arguments.first() else { return };
        let oxc_ast::ast::Argument::ObjectExpression(obj) = first_arg else { return };

        // Look for a `secret` key in that object.
        let has_secret = obj.properties.iter().any(|prop| {
            let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop else {
                return false;
            };
            match &p.key {
                oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str() == "secret",
                oxc_ast::ast::PropertyKey::StringLiteral(s) => s.value.as_str() == "secret",
                _ => false,
            }
        });
        if !has_secret {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`secret` duplicates `BETTER_AUTH_SECRET` \u{2014} remove it and use the env var.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
