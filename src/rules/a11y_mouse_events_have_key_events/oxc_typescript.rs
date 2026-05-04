//! a11y-mouse-events-have-key-events OxcCheck backend.
//!
//! Flags `onMouseOver` without `onFocus` and `onMouseOut` without `onBlur`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["onMouseOver", "onMouseOut"])
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

        let mut has_mouse_over = false;
        let mut has_mouse_out = false;
        let mut has_focus = false;
        let mut has_blur = false;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name) = &attr.name else {
                continue;
            };
            match name.name.as_str() {
                "onMouseOver" => has_mouse_over = true,
                "onMouseOut" => has_mouse_out = true,
                "onFocus" => has_focus = true,
                "onBlur" => has_blur = true,
                _ => {}
            }
        }

        let offset = opening.span.start as usize;

        if has_mouse_over && !has_focus {
            let (line, column) = byte_offset_to_line_col(ctx.source, offset);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`onMouseOver` must be accompanied by `onFocus` for keyboard accessibility.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }

        if has_mouse_out && !has_blur {
            let (line, column) = byte_offset_to_line_col(ctx.source, offset);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`onMouseOut` must be accompanied by `onBlur` for keyboard accessibility.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
