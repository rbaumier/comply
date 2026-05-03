//! html-require-explicit-size OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXAttributeItem;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["img", "video"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };

        let tag_text = &ctx.source[opening.name.span().start as usize..opening.name.span().end as usize];
        if tag_text != "img" && tag_text != "video" {
            return;
        }

        let mut has_width = false;
        let mut has_height = false;
        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else { continue };
            let name_text = &ctx.source[attr.name.span().start as usize..attr.name.span().end as usize];
            match name_text {
                "width" => has_width = true,
                "height" => has_height = true,
                _ => {}
            }
        }
        if has_width && has_height {
            return;
        }

        let missing = match (has_width, has_height) {
            (false, false) => "`width` and `height`",
            (false, true) => "`width`",
            (true, false) => "`height`",
            _ => unreachable!(),
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("`<{tag_text}>` is missing {missing} — causes layout shift."),
            severity: Severity::Warning,
            span: None,
        });
    }
}
