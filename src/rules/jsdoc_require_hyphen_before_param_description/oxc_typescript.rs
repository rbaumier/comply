//! jsdoc/require-hyphen-before-param-description OXC backend — flag `@param`
//! tags missing a ` - ` separator between name and description.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::jsdoc_helpers::scan_blocks;
use std::sync::Arc;

pub struct Check;

/// Returns true if the param has a description but no ` - ` separator.
fn missing_hyphen(body: &str) -> bool {
    let after_type = strip_leading_type(body);
    let mut it = after_type.splitn(2, char::is_whitespace);
    let Some(_name) = it.next() else {
        return false;
    };
    let Some(tail) = it.next() else {
        return false; // no description at all
    };
    let tail = tail.trim_start();
    if tail.is_empty() {
        return false;
    }
    !tail.starts_with('-')
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
                    if !matches!(tag.name.as_str(), "param" | "arg" | "argument") {
                        continue;
                    }
                    if missing_hyphen(&tag.body) {
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line: tag.line + base_line,
                            column: 1,
                            rule_id: super::META.id.into(),
                            message:
                                "Insert a `-` between the @param name and its description for readability."
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
