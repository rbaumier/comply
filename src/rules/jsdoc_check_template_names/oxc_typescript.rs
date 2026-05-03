//! jsdoc/check-template-names OXC backend — scan JSDoc comments for
//! `@template T` entries whose `T` is never referenced in another tag.

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
            // Reconstruct the full comment text including delimiters.
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            let Some(raw) = ctx.source.get(start..end) else { continue };
            // Only process JSDoc-style `/** ... */` comments.
            if !raw.starts_with("**") {
                continue;
            }
            let text = format!("/*{raw}*/");

            for block in scan_blocks(&text) {
                let tags = block.tags();
                let haystack: String = tags
                    .iter()
                    .filter(|t| t.name != "template")
                    .map(|t| t.body.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");

                for tag in tags.iter().filter(|t| t.name == "template") {
                    let names = extract_template_names(&tag.body);
                    for name in names {
                        if !contains_identifier(&haystack, &name) {
                            let (line, _) = byte_offset_to_line_col(ctx.source, start);
                            diagnostics.push(Diagnostic {
                                path: Arc::clone(&ctx.path_arc),
                                line: line + tag.line - 1,
                                column: 1,
                                rule_id: super::META.id.into(),
                                message: format!(
                                    "@template parameter `{name}` is declared but never referenced in the block."
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

fn extract_template_names(body: &str) -> Vec<String> {
    let after_type = strip_leading_type(body);
    let head = after_type.split(['-', ':']).next().unwrap_or("");
    head.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && is_ident(s))
        .collect()
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

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
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
