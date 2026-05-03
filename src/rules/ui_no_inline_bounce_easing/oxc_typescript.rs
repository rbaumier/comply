//! OxcCheck backend for ui-no-inline-bounce-easing — flag bounce/elastic
//! easing in inline JSX style objects.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Expression, JSXAttributeItem, JSXAttributeName, JSXExpression, ObjectPropertyKind,
    PropertyKey,
};
use std::sync::Arc;

pub struct Check;

const BOUNCE_NAMES: &[&str] = &["bounce", "elastic", "wobble", "jiggle", "spring"];

const TIMING_KEYS: &[&str] = &[
    "transition",
    "transitionTimingFunction",
    "animation",
    "animationTimingFunction",
];

const ANIM_NAME_KEYS: &[&str] = &["animation", "animationName"];

fn is_overshoot_cubic_bezier(value: &str) -> bool {
    let Some(start) = value.find("cubic-bezier(") else {
        return false;
    };
    let rest = &value[start + 13..];
    let Some(end) = rest.find(')') else {
        return false;
    };
    let inner = &rest[..end];
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() != 4 {
        return false;
    }
    let y1: f64 = parts[1].trim().parse().unwrap_or(0.0);
    let y2: f64 = parts[3].trim().parse().unwrap_or(0.0);
    !(-0.1..=1.1).contains(&y1) || !(-0.1..=1.1).contains(&y2)
}

fn has_bounce_animation_name(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .any(|token| BOUNCE_NAMES.iter().any(|&name| token.starts_with(name)))
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["transitionTimingFunction", "animationTimingFunction"])
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

        // Find the `style` attribute.
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

            // style={{ ... }} — the value is a JSXExpressionContainer with an object.
            let Some(ref value) = attr.value else {
                continue;
            };
            let oxc_ast::ast::JSXAttributeValue::ExpressionContainer(container) = value else {
                continue;
            };
            let JSXExpression::ObjectExpression(obj) = &container.expression else {
                continue;
            };

            for prop in &obj.properties {
                let ObjectPropertyKind::ObjectProperty(pair) = prop else {
                    continue;
                };
                let key = match &pair.key {
                    PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                    _ => continue,
                };

                let is_timing = TIMING_KEYS.contains(&key);
                let is_anim_name = ANIM_NAME_KEYS.contains(&key);
                if !is_timing && !is_anim_name {
                    continue;
                }

                // Extract string value.
                let value_str = match &pair.value {
                    Expression::StringLiteral(s) => s.value.as_str(),
                    _ => continue,
                };

                let flagged = (is_timing && is_overshoot_cubic_bezier(value_str))
                    || (is_anim_name && has_bounce_animation_name(value_str));

                if !flagged {
                    continue;
                }

                let (line, column) =
                    byte_offset_to_line_col(ctx.source, pair.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message:
                        "Bounce/elastic easing — use `ease-out` or a smooth deceleration curve."
                            .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }

    #[test]
    fn flags_overshoot_cubic_bezier() {
        let src =
            r#"<div style={{ transition: 'all 0.3s cubic-bezier(0.68, -0.55, 0.27, 1.55)' }} />"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_bounce_animation_name() {
        let src = r#"<div style={{ animationName: 'bounceIn' }} />"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_elastic_keyword() {
        let src = r#"<div style={{ animation: '0.5s elastic ease-in' }} />"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_ease_out() {
        assert!(run(r#"<div style={{ transition: 'all 0.3s ease-out' }} />"#).is_empty());
    }

    #[test]
    fn allows_smooth_cubic_bezier() {
        let src =
            r#"<div style={{ transitionTimingFunction: 'cubic-bezier(0.16, 1, 0.3, 1)' }} />"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_timing_key() {
        assert!(run(r#"<div style={{ color: 'bounce' }} />"#).is_empty());
    }

    #[test]
    fn allows_non_style_object() {
        assert!(
            run(r#"const config = { transition: 'all 0.3s cubic-bezier(0.68, -0.55, 0.27, 1.55)' };"#)
                .is_empty()
        );
    }
}
