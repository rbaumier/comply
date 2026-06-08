use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use crate::rules::jsdoc_text_helpers::{
    find_jsdoc_blocks, parse_tags, strip_type_annotation, value_has_description,
};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["/**"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let source = ctx.source;

        // Scan source for `/**` comment blocks the same way the TreeSitter
        // version does (via comment node text).  We walk char-by-char to
        // find `/**` openings and their matching `*/` closings.
        let bytes = source.as_bytes();
        let len = bytes.len();
        let mut i = 0;
        while i + 2 < len {
            if bytes[i] == b'/' && bytes[i + 1] == b'*' && bytes[i + 2] == b'*' {
                // Find closing */
                let start = i;
                let mut j = i + 3;
                while j + 1 < len {
                    if bytes[j] == b'*' && bytes[j + 1] == b'/' {
                        j += 2;
                        break;
                    }
                    j += 1;
                }
                let comment_text = &source[start..j];
                let line_offset = byte_offset_to_line_col(source, start).0 - 1;

                for block in find_jsdoc_blocks(comment_text) {
                    for tag in parse_tags(&block.content) {
                        if tag.name != "next" {
                            continue;
                        }
                        let after_type = strip_type_annotation(&tag.value);
                        if !value_has_description(after_type) {
                            diagnostics.push(Diagnostic {
                                path: Arc::clone(&ctx.path_arc),
                                line: block.start_line + tag.line_offset + 1 + line_offset,
                                column: 1,
                                rule_id: super::META.id.into(),
                                message: "@next tag is missing a description — explain what the caller passes to .next().".into(),
                                severity: Severity::Warning,
                                span: None,
                            });
                        }
                    }
                }
                i = j;
            } else {
                i += 1;
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
    fn flags_next_without_description() {
        let src = "/**\n * @next {string}\n */\nfunction* g() { yield 1; }";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_bare_next() {
        let src = "/**\n * @next\n */\nfunction* g() { yield 1; }";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_next_with_description() {
        let src =
            "/**\n * @next {string} the string passed into next()\n */\nfunction* g() { yield 1; }";
        assert!(run(src).is_empty());
    }
}
