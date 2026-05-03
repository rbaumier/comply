//! xstate-entry-exit-action OXC backend — validate that `entry`/`exit`
//! property values are a string, function, action creator call, or array.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, PropertyKey};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Accepted value shapes for `entry` / `exit`.
fn is_valid_action_value(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::StringLiteral(_)
            | Expression::TemplateLiteral(_)
            | Expression::Identifier(_)
            | Expression::CallExpression(_)
            | Expression::ArrowFunctionExpression(_)
            | Expression::FunctionExpression(_)
            | Expression::ArrayExpression(_)
    )
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["entry", "exit"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else {
            return;
        };

        let key_name = match &prop.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => return,
        };
        if key_name != "entry" && key_name != "exit" {
            return;
        }

        if is_valid_action_value(&prop.value) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, prop.value.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{key_name}` must be a string, function, action creator call, or array \u{2014} got an invalid value.",
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
