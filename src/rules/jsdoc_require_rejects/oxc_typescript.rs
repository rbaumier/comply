//! jsdoc/require-rejects OXC backend — async functions that can reject
//! must declare `@rejects`.
//!
//! Uses run_on_semantic to scan all nodes for leading JSDoc comments on
//! async functions containing `throw` or `Promise.reject(`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use crate::rules::jsdoc_text_helpers::{find_jsdoc_blocks, following_code, has_tag, parse_tags};
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
        // This rule is fundamentally text-based (JSDoc comment parsing),
        // so we scan the source directly for `/** ... */` blocks.
        let mut diagnostics = Vec::new();
        let source = ctx.source;
        let mut search_from = 0;

        while let Some(start) = source[search_from..].find("/**") {
            let abs_start = search_from + start;
            let Some(end_rel) = source[abs_start..].find("*/") else {
                break;
            };
            let abs_end = abs_start + end_rel + 2;
            let comment_text = &source[abs_start..abs_end];

            for block in find_jsdoc_blocks(comment_text) {
                let tags = parse_tags(&block.content);
                if has_tag(&tags, "rejects") || has_tag(&tags, "throws") {
                    continue;
                }
                let code = following_code(source, comment_text);
                if !is_async_fn(code) {
                    continue;
                }
                if !has_rejection_path(code) {
                    continue;
                }
                let (line, _column) =
                    byte_offset_to_line_col(source, abs_start);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line: line + block.start_line,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "Async function may reject — document it with `@rejects {ErrorType} when ...`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }

            search_from = abs_end;
        }
        diagnostics
    }
}

fn is_async_fn(code: &str) -> bool {
    let first_line = code
        .lines()
        .map(str::trim_start)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    first_line.starts_with("async ")
        || first_line.starts_with("export async ")
        || first_line.starts_with("export default async ")
        || first_line.contains(" async ")
}

fn has_rejection_path(code: &str) -> bool {
    code.contains("Promise.reject(") || code.contains("throw ")
}
