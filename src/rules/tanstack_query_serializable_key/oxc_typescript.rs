use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["queryKey"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectExpression(obj) = node.kind() else { return };

        for prop in &obj.properties {
            let ObjectPropertyKind::ObjectProperty(p) = prop else { continue };

            let key_name = match &p.key {
                PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                PropertyKey::StringLiteral(s) => s.value.as_str(),
                _ => continue,
            };
            if key_name != "queryKey" {
                continue;
            }

            let Expression::ArrayExpression(arr) = &p.value else { continue };

            for element in &arr.elements {
                let Some(expr) = element.as_expression() else { continue };
                let Some(reason) = unserializable_reason(expr) else { continue };
                let span = expr.span();
                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`queryKey` element is not serializable ({reason}). Convert it to a primitive before using it as a cache key."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
    }
}

fn unserializable_reason(expr: &Expression<'_>) -> Option<&'static str> {
    match expr {
        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => {
            Some("function/closure")
        }
        Expression::NewExpression(new_expr) => {
            if let Expression::Identifier(id) = &new_expr.callee {
                if id.name == "Date" {
                    return Some("`new Date()` — use `.toISOString()`");
                }
            }
            Some("class instance")
        }
        Expression::CallExpression(call) => {
            if let Expression::Identifier(id) = &call.callee {
                if id.name == "Symbol" {
                    return Some("`Symbol(...)`");
                }
            }
            None
        }
        _ => None,
    }
}
