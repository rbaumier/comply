//! html-no-skip-heading-levels OXC backend — flag skipped heading levels in JSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXElementName;
use std::sync::Arc;

pub struct Check;

fn get_heading_level(name: &str) -> Option<u8> {
    match name {
        "h1" => Some(1),
        "h2" => Some(2),
        "h3" => Some(3),
        "h4" => Some(4),
        "h5" => Some(5),
        "h6" => Some(6),
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut headings: Vec<(u8, u32)> = Vec::new();

        for node in semantic.nodes().iter() {
            if let AstKind::JSXOpeningElement(opening) = node.kind() {
                let tag = match &opening.name {
                    JSXElementName::Identifier(ident) => ident.name.as_str(),
                    _ => continue,
                };
                if let Some(level) = get_heading_level(tag) {
                    headings.push((level, opening.span.start));
                }
            }
        }

        if headings.is_empty() {
            return Vec::new();
        }

        // Sort by source position to process in document order.
        headings.sort_by_key(|&(_, offset)| offset);

        let mut diagnostics = Vec::new();
        let mut max_seen: u8 = 0;

        for &(level, offset) in &headings {
            if max_seen == 0 {
                max_seen = level;
                continue;
            }
            if level <= max_seen {
                max_seen = level;
                continue;
            }
            if level > max_seen + 1 {
                let (line, column) = byte_offset_to_line_col(ctx.source, offset as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Heading level h{level} skips from h{max_seen}. Use h{} instead.",
                        max_seen + 1
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            max_seen = level;
        }

        diagnostics
    }
}
