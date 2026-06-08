//! OXC backend for jsdoc/require-property-name.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::jsdoc_text_helpers::{find_jsdoc_blocks, parse_tags, strip_type_annotation};
use std::sync::Arc;

use oxc_ast::CommentKind;

fn property_tag_has_name(value: &str) -> bool {
    let rest = strip_type_annotation(value.trim());
    let first = rest.split_whitespace().next().unwrap_or("");
    !first.is_empty() && !first.starts_with('-')
}

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
        for comment in semantic.comments() {
            if comment.kind == CommentKind::Line {
                continue;
            }
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            if end > ctx.source.len() {
                continue;
            }
            let text = &ctx.source[start..end];
            if !text.starts_with("/**") {
                continue;
            }
            let (line_offset, _) = byte_offset_to_line_col(ctx.source, start);
            // line_offset is 1-based; find_jsdoc_blocks returns 0-based offsets
            // The original TS rule used node.start_position().row (0-based).
            let line_offset_0 = line_offset - 1;

            for block in find_jsdoc_blocks(text) {
                for tag in parse_tags(&block.content) {
                    if tag.name != "property" {
                        continue;
                    }
                    if !property_tag_has_name(&tag.value) {
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line: block.start_line + tag.line_offset + 1 + line_offset_0,
                            column: 1,
                            rule_id: super::META.id.into(),
                            message: "@property tag is missing a name — add an identifier after the type.".into(),
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

#[cfg(test)]
mod tests {
    use super::*;



    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }


    #[test]
    fn flags_property_without_name() {
        let src = "/**\n * @property {string}\n */\ntype T = {};";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_property_with_only_description() {
        let src = "/**\n * @property - some description\n */\ntype T = {};";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_property_with_name() {
        let src = "/**\n * @property {string} name\n */\ntype T = { name: string };";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_property_with_name_and_description() {
        let src =
            "/**\n * @property {string} name - the user's name\n */\ntype T = { name: string };";
        assert!(run(src).is_empty());
    }
}
