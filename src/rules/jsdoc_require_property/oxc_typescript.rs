//! jsdoc/require-property OXC backend — comment-based, uses semantic.comments().

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::jsdoc_helpers::scan_blocks;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for comment in semantic.comments() {
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            let Some(text) = ctx.source.get(start..end) else {
                continue;
            };
            if !text.starts_with("/**") {
                continue;
            }

            let (line_offset, _) = byte_offset_to_line_col(ctx.source, start);
            let line_offset = line_offset - 1;

            for block in scan_blocks(text) {
                let tags = block.tags();
                let Some(typedef) = tags.iter().find(|t| t.name == "typedef") else {
                    continue;
                };
                if !super::types_object(&typedef.body) {
                    continue;
                }
                let has_property = tags
                    .iter()
                    .any(|t| matches!(t.name.as_str(), "property" | "prop"));
                if !has_property {
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line: typedef.line + line_offset,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message:
                            "`@typedef` declares an object type but no `@property` entries \u{2014} document each field."
                                .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
        diagnostics
    }
}
