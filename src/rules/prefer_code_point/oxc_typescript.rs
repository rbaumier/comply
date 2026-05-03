//! prefer-code-point oxc backend — flag `charCodeAt` and `String.fromCharCode`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["charCodeAt", "fromCharCode"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
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

        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };

        let prop_name = member.property.name.as_str();
        match prop_name {
            "charCodeAt" => {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Prefer `String#codePointAt()` over `String#charCodeAt()`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            "fromCharCode" => {
                // Verify object is `String`
                let Expression::Identifier(obj) = &member.object else {
                    return;
                };
                if obj.name.as_str() != "String" {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Prefer `String.fromCodePoint()` over `String.fromCharCode()`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
    }
}
