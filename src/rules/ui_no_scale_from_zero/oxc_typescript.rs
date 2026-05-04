//! OxcCheck backend for ui-no-scale-from-zero.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeValue, JSXExpression, ObjectPropertyKind,
    PropertyKey,
};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Detect a `scale(0)` / `scale(0,0)` / `scale(0.0)` call.
fn references_scale_zero(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    for (i, _) in lower.match_indices("scale(") {
        let rest = &lower[i + 6..];
        let Some(end) = rest.find(')') else { continue };
        let inner = rest[..end].trim();
        let args: Vec<&str> = inner.split(',').map(str::trim).collect();
        if !args.is_empty()
            && args
                .iter()
                .all(|a| a.parse::<f64>().is_ok_and(|n| n == 0.0))
        {
            return true;
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["scale(0"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(jsx) = node.kind() else { return };

        // Find `style={{ ... }}` attribute
        for attr in &jsx.attributes {
            let JSXAttributeItem::Attribute(attr) = attr else { continue };
            let name = &ctx.source[attr.name.span().start as usize..attr.name.span().end as usize];
            if name != "style" {
                continue;
            }
            let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value else {
                continue;
            };
            let JSXExpression::ObjectExpression(obj) = &container.expression else {
                continue;
            };

            // Look for `transform: 'scale(0)'` in the style object
            for prop in &obj.properties {
                let ObjectPropertyKind::ObjectProperty(p) = prop else { continue };
                let key_name = match &p.key {
                    PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                    _ => continue,
                };
                if key_name != "transform" {
                    continue;
                }
                let value_text =
                    &ctx.source[p.value.span().start as usize..p.value.span().end as usize];
                let value = value_text.trim_matches(|c| c == '"' || c == '\'');

                if references_scale_zero(value) {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, p.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "`scale(0)` causes subpixel rendering blur and makes elements appear from \
                                  nowhere — use `scale(0.95)` with `opacity: 0` instead."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
    }
}
