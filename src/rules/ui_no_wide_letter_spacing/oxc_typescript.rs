//! OxcCheck backend for ui-no-wide-letter-spacing.
//!
//! Flags inline `letterSpacing` string values in `em` above 0.05 inside
//! JSX `style` attributes.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn parse_em(raw: &str) -> Option<f64> {
    let cleaned = raw.trim_matches(|c| c == '"' || c == '\'').trim();
    let stripped = cleaned.strip_suffix("em")?;
    if stripped.ends_with('r') {
        return None;
    }
    stripped.trim().parse::<f64>().ok()
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["letterSpacing"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        use oxc_ast::ast::*;

        let AstKind::JSXOpeningElement(el) = node.kind() else {
            return;
        };

        // Find the `style` attribute
        for item in el.attributes.iter() {
            let JSXAttributeItem::Attribute(attr) = item else {
                continue;
            };
            let JSXAttributeName::Identifier(id) = &attr.name else {
                continue;
            };
            if id.name.as_str() != "style" {
                continue;
            }

            // style={{ letterSpacing: '0.1em' }}
            let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value else {
                continue;
            };
            let JSXExpression::ObjectExpression(obj) =
                &container.expression
            else {
                continue;
            };

            for prop in &obj.properties {
                let ObjectPropertyKind::ObjectProperty(pair) = prop else {
                    continue;
                };

                // Check key is `letterSpacing`
                let key_name = match &pair.key {
                    PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                    PropertyKey::StringLiteral(s) => s.value.as_str(),
                    _ => continue,
                };
                if key_name != "letterSpacing" {
                    continue;
                }

                // Value must be a string literal
                let Expression::StringLiteral(val) = &pair.value else {
                    continue;
                };
                let raw = val.value.as_str();
                let Some(num) = parse_em(raw) else {
                    continue;
                };

                let max_spacing = ctx.config.float(
                    "ui-no-wide-letter-spacing",
                    "max_letter_spacing_em",
                    ctx.lang,
                );
                if num <= max_spacing {
                    continue;
                }

                let (line, column) =
                    byte_offset_to_line_col(ctx.source, pair.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`letterSpacing: \"{raw}\"` — values above {max_spacing}em hurt readability."
                    ),
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
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }

    #[test]
    fn flags_0_1_em() {
        assert_eq!(run(r#"<p style={{ letterSpacing: '0.1em' }} />"#).len(), 1);
    }

    #[test]
    fn flags_0_2_em() {
        assert_eq!(run(r#"<p style={{ letterSpacing: '0.2em' }} />"#).len(), 1);
    }

    #[test]
    fn allows_0_03_em() {
        assert!(run(r#"<p style={{ letterSpacing: '0.03em' }} />"#).is_empty());
    }

    #[test]
    fn allows_pixel_value() {
        assert!(run(r#"<p style={{ letterSpacing: '2px' }} />"#).is_empty());
    }

    #[test]
    fn allows_non_style_object() {
        assert!(run(r#"const config = { letterSpacing: '0.2em' };"#).is_empty());
    }
}
