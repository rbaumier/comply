//! jsdoc/check-template-names OXC backend — scan JSDoc comments for
//! `@template T` entries whose `T` is never referenced in another tag.

use crate::diagnostic::{Diagnostic, Severity};
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
            // `Comment::span` covers the comment with its delimiters, so
            // `text` is the block verbatim and an offset inside it maps back to
            // the file by adding the comment's own start offset.
            let comment_offset = comment.span.start as usize;
            let end = comment.span.end as usize;
            let Some(text) = ctx.source.get(comment_offset..end) else { continue };
            // Only process JSDoc-style `/** ... */` comments.
            if !text.starts_with("/**") {
                continue;
            }

            for block in scan_blocks(text) {
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
                            // `@template T, U` declares several parameters on
                            // one line: anchor on the name the message quotes,
                            // so two unused parameters are two distinct columns.
                            let offset = template_name_offset(text, tag.line, &name)
                                .map_or(comment_offset, |o| comment_offset + o);
                            diagnostics.push(Diagnostic::at_offset(
                                Arc::clone(&ctx.path_arc),
                                ctx.source,
                                (offset, name.len()),
                                super::META.id,
                                format!(
                                    "@template parameter `{name}` is declared but never referenced in the block."
                                ),
                                Severity::Error,
                            ));
                        }
                    }
                }
            }
        }
        diagnostics
    }
}

/// Byte offset of `name` inside the comment text `block_text`, on the 1-based
/// `tag_line` where the `@template` header sits. `None` when the name is not on
/// that line — a wrapped `@template` continuation line, where the caller falls
/// back to the comment itself.
fn template_name_offset(block_text: &str, tag_line: usize, name: &str) -> Option<usize> {
    let line_start: usize = block_text
        .split_inclusive('\n')
        .take(tag_line.saturating_sub(1))
        .map(str::len)
        .sum();
    let line = block_text.get(line_start..)?.split('\n').next()?;
    find_identifier(line, name).map(|column| line_start + column)
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
    find_identifier(hay, needle).is_some()
}

/// Byte offset of the first occurrence of `needle` in `hay` that stands as a
/// whole identifier — neither neighbour is an identifier byte, so `T` does not
/// match inside `Then`.
fn find_identifier(hay: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return None;
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
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_by_id(super::super::META.id, source, "t.ts")
    }

    #[test]
    fn anchors_each_unused_parameter_on_its_own_name() {
        // Regression for rbaumier/comply#8386 — `@template T, U` declares two
        // parameters on one line, and both diagnostics used to land on column 1.
        let src = "/**\n * @template T, U\n * @param {number} n\n */\nfunction f(n) {}\n";
        let diags = run_on(src);
        let positions: Vec<(usize, usize)> = diags.iter().map(|d| (d.line, d.column)).collect();
        assert_eq!(positions, vec![(2, 14), (2, 17)]);
        let anchored: Vec<&str> = diags
            .iter()
            .map(|d| {
                let (offset, len) = d.span.expect("the anchor carries the name's span");
                &src[offset..offset + len]
            })
            .collect();
        assert_eq!(anchored, vec!["T", "U"]);
    }
}
