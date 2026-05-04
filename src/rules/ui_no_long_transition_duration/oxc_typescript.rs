//! ui-no-long-transition-duration OXC backend — flag inline `transitionDuration`
//! / `animationDuration` string values exceeding 1 second.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Expression, JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXExpression,
    ObjectPropertyKind, PropertyKey,
};
use std::sync::Arc;

pub struct Check;

/// Parse a CSS duration literal. Returns the value in milliseconds when the
/// duration exceeds the configured threshold, or `None` otherwise.
fn parse_excessive_duration(raw: &str, max_ms: f64, max_s: f64) -> Option<f64> {
    let cleaned = raw.trim_matches(|c| c == '"' || c == '\'').trim();
    if let Some(stripped) = cleaned.strip_suffix("ms") {
        let num = stripped.trim().parse::<f64>().ok()?;
        if num > max_ms {
            return Some(num);
        }
        return None;
    }
    if let Some(stripped) = cleaned.strip_suffix('s') {
        let num = stripped.trim().parse::<f64>().ok()?;
        if num > max_s {
            return Some(num * 1000.0);
        }
    }
    None
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["transitionDuration", "animationDuration"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };

        // Find the `style` attribute
        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(attr_name) = &attr.name else {
                continue;
            };
            if attr_name.name.as_str() != "style" {
                continue;
            }
            // style={{ ... }} — value is a JSX expression containing an object
            let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value else {
                continue;
            };
            let JSXExpression::ObjectExpression(obj) = &container.expression else {
                continue;
            };
            let max_ms = ctx
                .config
                .float("ui-no-long-transition-duration", "max_duration_ms", ctx.lang);
            let max_s = ctx
                .config
                .float("ui-no-long-transition-duration", "max_duration_s", ctx.lang);

            for prop in &obj.properties {
                let ObjectPropertyKind::ObjectProperty(p) = prop else {
                    continue;
                };
                let key_name = match &p.key {
                    PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                    PropertyKey::StringLiteral(s) => s.value.as_str(),
                    _ => continue,
                };
                if key_name != "transitionDuration" && key_name != "animationDuration" {
                    continue;
                }
                let Expression::StringLiteral(val) = &p.value else {
                    continue;
                };
                let raw = val.value.as_str();
                let Some(ms) = parse_excessive_duration(raw, max_ms, max_s) else {
                    continue;
                };
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, p.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{key_name}: \"{raw}\"` ({ms}ms) \u{2014} durations above 1s feel sluggish and block \
                         interaction."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}
