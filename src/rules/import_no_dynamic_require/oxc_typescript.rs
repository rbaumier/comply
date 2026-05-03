//! import-no-dynamic-require OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["require"])
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

        // Callee must be a bare `require` identifier
        let Expression::Identifier(ident) = &call.callee else {
            return;
        };
        if ident.name.as_str() != "require" {
            return;
        }

        let Some(first_arg) = call.arguments.first() else {
            return;
        };

        // Check if the argument is a static string literal
        match first_arg {
            Argument::StringLiteral(_) => return, // static
            Argument::TemplateLiteral(tpl) => {
                // Template strings with no expressions are static
                if tpl.expressions.is_empty() {
                    return;
                }
            }
            _ => {}
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Calls to `require()` should use string literals.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
