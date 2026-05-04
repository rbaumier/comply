//! tanstack-start-loader-stale-time OXC backend — flag `ensureQueryData(...)`
//! calls whose options object is missing `staleTime` or sets it below the
//! configured minimum.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

pub struct Check;

/// Find the `staleTime` property value inside an object expression.
/// Returns `Some(numeric_value)` if found and is a number literal,
/// `None` if the property is missing, or `Some(u64::MAX)` for non-literal values.
fn find_stale_time_value(obj: &oxc_ast::ast::ObjectExpression) -> Option<u64> {
    for prop in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            continue;
        };
        let key_name = match &p.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => continue,
        };
        if key_name != "staleTime" {
            continue;
        }
        // Check if value is a numeric literal.
        if let Expression::NumericLiteral(num) = &p.value {
            return Some(num.value as u64);
        }
        // Non-literal value — assume OK (could be a constant).
        return Some(u64::MAX);
    }
    None
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["ensureQueryData"])
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

        // Match `ensureQueryData(...)` or `<receiver>.ensureQueryData(...)`.
        let is_ensure = match &call.callee {
            Expression::Identifier(id) => id.name.as_str() == "ensureQueryData",
            Expression::StaticMemberExpression(member) => {
                member.property.name.as_str() == "ensureQueryData"
            }
            _ => false,
        };
        if !is_ensure {
            return;
        }

        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Some(first_expr) = first_arg.as_expression() else {
            return;
        };
        let Expression::ObjectExpression(obj) = first_expr else {
            return;
        };

        let min_stale_time = ctx.config.threshold(
            "tanstack-start-loader-stale-time",
            "min_stale_time_ms",
            ctx.lang,
        ) as u64;

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);

        match find_stale_time_value(obj) {
            Some(n) if n < min_stale_time => {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`staleTime: {n}` is below {min_stale_time}ms — loader data will refetch during navigation."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            None => {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`ensureQueryData` call is missing `staleTime` — set it to at least {min_stale_time}ms to avoid refetches during navigation."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {} // Value at or above threshold.
        }
    }
}
