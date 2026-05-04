//! react-checked-requires-onchange OxcCheck backend.
//!
//! Flags `<input checked={...} />` without `onChange` or `readOnly`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXElementName};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["checked"])
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

        // Must be an <input> tag.
        let tag = match &opening.name {
            JSXElementName::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if tag != "input" {
            return;
        }

        let mut has_checked = false;
        let mut has_default_checked = false;
        let mut has_on_change = false;
        let mut has_read_only = false;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name) = &attr.name else {
                continue;
            };
            match name.name.as_str() {
                "checked" => has_checked = true,
                "defaultChecked" => has_default_checked = true,
                "onChange" => has_on_change = true,
                "readOnly" => has_read_only = true,
                _ => {}
            }
        }

        if !has_checked || has_default_checked || has_on_change || has_read_only {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`checked` without `onChange` or `readOnly` renders \
                      a frozen input. Add an `onChange` handler or \
                      `readOnly`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
