//! perf-img-fetchpriority-high OXC backend — flag hero `<img>` without
//! `fetchpriority="high"`, and reject conflicting high + lazy combos.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXExpression,
};
use std::sync::Arc;

pub struct Check;

fn parse_dim(val: &str) -> Option<u32> {
    val.trim().trim_end_matches("px").trim().parse::<u32>().ok()
}

fn jsx_attr_string_value<'a>(attr: &'a oxc_ast::ast::JSXAttribute<'a>) -> Option<&'a str> {
    match attr.value.as_ref()? {
        JSXAttributeValue::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

fn jsx_attr_numeric_expr(attr: &oxc_ast::ast::JSXAttribute) -> Option<u32> {
    let JSXAttributeValue::ExpressionContainer(container) = attr.value.as_ref()? else {
        return None;
    };
    let JSXExpression::NumericLiteral(n) = &container.expression else { return None };
    Some(n.value as u32)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["img"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };

        let oxc_ast::ast::JSXElementName::Identifier(tag) = &opening.name else { return };
        if tag.name.as_str() != "img" {
            return;
        }

        let mut fetchpriority: Option<String> = None;
        let mut loading: Option<String> = None;
        let mut width: Option<u32> = None;
        let mut height: Option<u32> = None;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else { continue };
            let JSXAttributeName::Identifier(name) = &attr.name else { continue };
            match name.name.as_str() {
                "fetchpriority" => {
                    fetchpriority = jsx_attr_string_value(attr).map(str::to_owned);
                }
                "loading" => {
                    loading = jsx_attr_string_value(attr).map(str::to_owned);
                }
                "width" => {
                    if let Some(v) = jsx_attr_string_value(attr).and_then(parse_dim) {
                        width = Some(v);
                    } else if let Some(v) = jsx_attr_numeric_expr(attr) {
                        width = Some(v);
                    }
                }
                "height" => {
                    if let Some(v) = jsx_attr_string_value(attr).and_then(parse_dim) {
                        height = Some(v);
                    } else if let Some(v) = jsx_attr_numeric_expr(attr) {
                        height = Some(v);
                    }
                }
                _ => {}
            }
        }

        // Case 1: conflicting fetchpriority="high" + loading="lazy"
        if fetchpriority.as_deref() == Some("high") && loading.as_deref() == Some("lazy") {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, opening.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`<img>` with `fetchpriority=\"high\"` must not also set `loading=\"lazy\"` \u{2014} they contradict each other.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }

        // Case 2: hero-sized img without fetchpriority="high"
        let hero_threshold = ctx
            .config
            .threshold("perf-img-fetchpriority-high", "hero_pixel_threshold", ctx.lang)
            as u32;
        let is_hero = width.is_some_and(|w| w >= hero_threshold)
            || height.is_some_and(|h| h >= hero_threshold);
        if is_hero && fetchpriority.as_deref() != Some("high") {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, opening.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Hero-sized `<img>` should declare `fetchpriority=\"high\"` so the browser starts fetching it early.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
