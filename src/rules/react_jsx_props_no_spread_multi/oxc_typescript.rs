//! react-jsx-props-no-spread-multi oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXAttributeItem;
use oxc_span::GetSpan;
use rustc_hash::FxHashSet;
use std::sync::Arc;

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

        let mut seen_spreads = FxHashSet::default();

        for attr_item in &opening.attributes {
            let JSXAttributeItem::SpreadAttribute(spread) = attr_item else {
                continue;
            };

            // Get source text of the spread argument
            let arg_span = spread.argument.span();
            let arg_start = arg_span.start as usize;
            let arg_end = arg_span.end as usize;
            if arg_end > ctx.source.len() {
                continue;
            }
            let arg_text = &ctx.source[arg_start..arg_end];

            if !seen_spreads.insert(arg_text.to_string()) {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, spread.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("`{arg_text}` is spread multiple times on this element."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}
