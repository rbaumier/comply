//! tailwind-min-touch-target oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXAttributeItem;
use std::sync::Arc;

const INTERACTIVE_TAGS: &[&str] = &["button", "a"];

/// Parse the numeric value from a spacing utility like `px-2`, `py-1`, `p-0`.
fn parse_spacing_value(tok: &str, prefix: &str) -> Option<u32> {
    let rest = tok.strip_prefix(prefix)?;
    rest.parse::<u32>().ok()
}

/// Does this className explicitly set an adequate height/width?
fn has_explicit_size(classes: &str) -> bool {
    classes.split_whitespace().any(|tok| {
        let base = tok.rsplit(':').next().unwrap_or(tok);
        for prefix in &["h-", "w-", "min-h-", "min-w-", "size-"] {
            if let Some(rest) = base.strip_prefix(prefix) {
                if rest == "full" || rest == "screen" {
                    return true;
                }
                if let Ok(n) = rest.parse::<u32>() {
                    if n >= 11 {
                        return true;
                    }
                }
            }
        }
        false
    })
}

/// Is the padding too small for a touch target?
fn padding_too_small(classes: &str) -> bool {
    let mut py = u32::MAX;
    let mut px = u32::MAX;

    for tok in classes.split_whitespace() {
        let base = tok.rsplit(':').next().unwrap_or(tok);
        if let Some(v) = parse_spacing_value(base, "p-") {
            py = py.min(v);
            px = px.min(v);
        }
        if let Some(v) = parse_spacing_value(base, "py-") {
            py = py.min(v);
        }
        if let Some(v) = parse_spacing_value(base, "px-") {
            px = px.min(v);
        }
        if let Some(v) = parse_spacing_value(base, "pt-") {
            py = py.min(v);
        }
        if let Some(v) = parse_spacing_value(base, "pb-") {
            py = py.min(v);
        }
    }

    py != u32::MAX && px != u32::MAX && py < 3 && px < 3
}

fn get_jsx_attr_string_value<'a>(
    attr: &'a oxc_ast::ast::JSXAttribute<'a>,
    source: &'a str,
) -> Option<&'a str> {
    let value = attr.value.as_ref()?;
    match value {
        oxc_ast::ast::JSXAttributeValue::StringLiteral(lit) => Some(lit.value.as_str()),
        oxc_ast::ast::JSXAttributeValue::ExpressionContainer(container) => {
            // For template literals / string expressions, fall back to source text.
            let span = container.span;
            let text = &source[span.start as usize..span.end as usize];
            // Strip {" and "} or {' and '}
            let inner = text.strip_prefix("{\"")?.strip_suffix("\"}")?;
            Some(inner)
        }
        _ => None,
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
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

        let tag = match &opening.name {
            oxc_ast::ast::JSXElementName::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        let lower = tag.to_ascii_lowercase();

        let mut class_value: Option<&str> = None;
        let mut is_role_button = false;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let oxc_ast::ast::JSXAttributeName::Identifier(name) = &attr.name else {
                continue;
            };
            match name.name.as_str() {
                "className" | "class" => {
                    class_value = get_jsx_attr_string_value(attr, ctx.source);
                }
                "role" => {
                    if let Some(val) = get_jsx_attr_string_value(attr, ctx.source) {
                        if val == "button" {
                            is_role_button = true;
                        }
                    }
                }
                _ => {}
            }
        }

        let interactive = INTERACTIVE_TAGS.contains(&lower.as_str()) || is_role_button;
        if !interactive {
            return;
        }

        let classes = class_value.unwrap_or("");
        if has_explicit_size(classes) {
            return;
        }
        if !padding_too_small(classes) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Interactive element below the ~44x44px touch target (WCAG 2.5.5). Use `h-11` + sufficient padding, or `size-11` for icon buttons.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
