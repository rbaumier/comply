//! jsdoc/require-returns-description OXC backend — flag `@returns` tags missing a description.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::jsdoc_helpers::scan_blocks;
use std::sync::Arc;

pub struct Check;

fn has_description(body: &str) -> bool {
    let after_type = strip_leading_type(body);
    let tail = after_type.trim_start_matches(|c: char| c == '-' || c == ':' || c.is_whitespace());
    !tail.trim().is_empty()
}

fn strip_leading_type(body: &str) -> &str {
    let trimmed = body.trim_start();
    if !trimmed.starts_with('{') {
        return trimmed;
    }
    let mut depth = 0usize;
    for (i, ch) in trimmed.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return trimmed[i + 1..].trim_start();
                }
            }
            _ => {}
        }
    }
    trimmed
}

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
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            if start < 2 {
                continue;
            }
            let doc_start = start - 2;
            let Some(raw) = ctx.source.get(doc_start..end) else {
                continue;
            };
            if !raw.starts_with("/**") {
                continue;
            }

            let (base_line, _) = byte_offset_to_line_col(ctx.source, doc_start);

            for block in scan_blocks(raw) {
                for tag in block.tags() {
                    if !matches!(tag.name.as_str(), "returns" | "return") {
                        continue;
                    }
                    if !has_description(&tag.body) {
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line: tag.line + base_line,
                            column: 1,
                            rule_id: super::META.id.into(),
                            message:
                                "`@returns` is missing a description — explain what the return value represents."
                                    .into(),
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
    fn allows_returns_with_description() {
        let src = "/**\n * @returns {string} the normalized name\n */\n";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_returns_without_type() {
        let src = "/**\n * @returns the normalized name\n */\n";
        assert!(run(src).is_empty());
    }
}
