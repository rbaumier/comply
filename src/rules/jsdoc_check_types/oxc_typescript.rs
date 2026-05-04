//! jsdoc/check-types oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::jsdoc_helpers::scan_blocks;
use std::sync::Arc;

const PREFERENCES: &[(&str, &str)] = &[
    ("String", "string"),
    ("Number", "number"),
    ("Boolean", "boolean"),
    ("Symbol", "symbol"),
    ("Bigint", "bigint"),
    ("BigInt", "bigint"),
    ("Object", "object"),
];

/// Extract the first balanced `{...}` group from a tag body.
fn extract_type_expr(body: &str) -> Option<&str> {
    let trimmed = body.trim_start();
    if !trimmed.starts_with('{') {
        return None;
    }
    let mut depth = 0usize;
    let bytes = trimmed.as_bytes();
    let mut start = None;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'{' {
            if depth == 0 {
                start = Some(i + 1);
            }
            depth += 1;
        } else if b == b'}' {
            depth -= 1;
            if depth == 0 {
                let s = start?;
                return Some(&trimmed[s..i]);
            }
        }
    }
    None
}

fn contains_identifier(hay: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let bytes = hay.as_bytes();
    let n = needle.as_bytes();
    let mut i = 0;
    while i + n.len() <= bytes.len() {
        if &bytes[i..i + n.len()] == n {
            let before_ok = i == 0 || !is_ident_byte(bytes[i - 1]);
            let after_idx = i + n.len();
            let after_ok = after_idx == bytes.len() || !is_ident_byte(bytes[after_idx]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
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
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            let Some(text) = ctx.source.get(start..end) else {
                continue;
            };
            if !text.starts_with("/*") {
                continue;
            }

            // Compute the line offset for this comment.
            let (line_offset, _) = byte_offset_to_line_col(ctx.source, start);

            for block in scan_blocks(text) {
                for tag in block.tags() {
                    let Some(type_expr) = extract_type_expr(&tag.body) else {
                        continue;
                    };
                    for (bad, good) in PREFERENCES {
                        if contains_identifier(type_expr, bad) {
                            diagnostics.push(Diagnostic {
                                path: Arc::clone(&ctx.path_arc),
                                line: tag.line + line_offset - 1,
                                column: 1,
                                rule_id: super::META.id.into(),
                                message: format!(
                                    "JSDoc type `{bad}` refers to the wrapper object — use lowercase `{good}` instead."
                                ),
                                severity: Severity::Warning,
                                span: None,
                            });
                        }
                    }
                }
            }
        }
        diagnostics
    }
}
