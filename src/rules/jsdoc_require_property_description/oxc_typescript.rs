//! jsdoc/require-property-description oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::jsdoc_text_helpers::{find_jsdoc_blocks, parse_tags, property_tag_has_description};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["/**"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for comment in semantic.comments().iter() {
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            let Some(raw) = ctx.source.get(start..end) else {
                continue;
            };
            if !raw.starts_with("/**") {
                continue;
            }
            let (line_offset, _) = byte_offset_to_line_col(ctx.source, start);

            for block in find_jsdoc_blocks(raw) {
                for tag in parse_tags(&block.content) {
                    if tag.name != "property" {
                        continue;
                    }
                    if !property_tag_has_description(&tag.value) {
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line: block.start_line + tag.line_offset + line_offset,
                            column: 1,
                            rule_id: super::META.id.into(),
                            message: "@property tag is missing a description — document what the property represents.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
            }
        }
        diagnostics
    }
}
