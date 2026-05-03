//! no-console-spaces OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const CONSOLE_METHODS: &[&str] = &["log", "debug", "info", "warn", "error"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["console"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let Expression::Identifier(obj) = &member.object else { return };
        if obj.name.as_str() != "console" {
            return;
        }
        let method = member.property.name.as_str();
        if !CONSOLE_METHODS.contains(&method) {
            return;
        }
        let arg_count = call.arguments.len();
        if arg_count < 2 {
            return;
        }

        for (i, arg) in call.arguments.iter().enumerate() {
            let val = match arg {
                oxc_ast::ast::Argument::StringLiteral(s) => s.value.as_str(),
                _ => continue,
            };
            if val.is_empty() {
                continue;
            }

            let is_first = i == 0;
            let is_last = i == arg_count - 1;

            // Leading single space in non-first arg.
            if !is_first && val.len() > 1 && val.starts_with(' ') && !val.starts_with("  ") {
                let span = match arg {
                    oxc_ast::ast::Argument::StringLiteral(s) => s.span,
                    _ => continue,
                };
                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "no-console-spaces".into(),
                    message: "Do not use leading space between `console` parameters.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }

            // Trailing single space in non-last arg.
            if !is_last && val.len() > 1 && val.ends_with(' ') && !val.ends_with("  ") {
                let span = match arg {
                    oxc_ast::ast::Argument::StringLiteral(s) => s.span,
                    _ => continue,
                };
                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "no-console-spaces".into(),
                    message: "Do not use trailing space between `console` parameters.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}
