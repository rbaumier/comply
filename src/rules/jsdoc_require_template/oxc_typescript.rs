//! jsdoc/require-template OXC backend — comment-based, uses semantic.comments().

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::jsdoc_text_helpers::{find_jsdoc_blocks, following_code, has_tag, parse_tags};
use std::sync::Arc;

pub struct Check;

/// Detect a `<T, U>` generics block in a function/class signature.
fn has_generics(code: &str) -> bool {
    let first_line = code
        .lines()
        .map(str::trim_start)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let head = match first_line.find('(') {
        Some(i) => &first_line[..i],
        None => first_line,
    };
    let open = match head.rfind('<') {
        Some(i) => i,
        None => return false,
    };
    let close = match head[open..].find('>') {
        Some(i) => open + i,
        None => return false,
    };
    let between = &head[open + 1..close];
    !between.trim().is_empty()
        && between.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, ',' | ' ' | '_' | '=' | '|' | '{' | '}' | ':' | '<' | '>')
        })
}

fn starts_with_function_or_class(code: &str) -> bool {
    let first = code
        .lines()
        .map(str::trim_start)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    first.starts_with("function ")
        || first.starts_with("async function ")
        || first.starts_with("export function ")
        || first.starts_with("export async function ")
        || first.starts_with("export default function ")
        || first.starts_with("class ")
        || first.starts_with("export class ")
        || first.starts_with("export default class ")
}

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
            // line_offset is 1-based from byte_offset_to_line_col, convert to 0-based for offset
            let line_offset = line_offset - 1;

            for block in find_jsdoc_blocks(text) {
                let tags = parse_tags(&block.content);
                if has_tag(&tags, "template") {
                    continue;
                }
                let code = following_code(ctx.source, text);
                if !starts_with_function_or_class(code) {
                    continue;
                }
                if !has_generics(code) {
                    continue;
                }
                let (line, column) = (block.start_line + 1 + line_offset, 1);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message:
                        "Generic signature has no `@template` tag \u{2014} document each type parameter."
                            .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}
