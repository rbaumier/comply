//! jsdoc-reject-function-type — oxc backend.
//!
//! Scans JSDoc comments for bare `{Function}` or `{function}` types.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn find_bare_function_types_in_line(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let start = i;
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b'}' {
                j += 1;
            }
            if j < bytes.len() {
                let type_content = line[start + 1..j].trim();
                if type_content == "Function" || type_content == "function" {
                    hits.push(start);
                }
            }
        }
        i += 1;
    }
    hits
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
            // OXC comment spans exclude the leading `//` or `/*`, so we need
            // to check the raw source including the prefix.
            let prefix_start = start.saturating_sub(2);
            let raw_prefix = ctx.source.get(prefix_start..start).unwrap_or("");
            if raw_prefix != "/*" {
                continue;
            }
            // Check next char for `*` (JSDoc `/** ... */`).
            let Some(text) = ctx.source.get(start..end) else { continue };
            if !text.starts_with('*') {
                continue;
            }

            // Compute the line/col of the comment start (the `/*` prefix).
            let (base_line, _) = byte_offset_to_line_col(ctx.source, prefix_start);

            for (line_idx, line) in text.lines().enumerate() {
                for col in find_bare_function_types_in_line(line) {
                    let abs_line = base_line + line_idx;
                    let abs_col = if line_idx == 0 {
                        // First line: offset from prefix_start
                        let (_, base_col) = byte_offset_to_line_col(ctx.source, prefix_start);
                        // +2 for `/*`, then +col within the inner text
                        base_col + 2 + col
                    } else {
                        col + 1
                    };
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line: abs_line,
                        column: abs_col,
                        rule_id: super::META.id.into(),
                        message: "JSDoc uses bare `Function` type \u{2014} provide a specific function signature instead.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn allows_specific_signature() {
        let src = "/**\n * @param {(x: string) => void} cb\n */\nfunction f(cb) {}";
        assert!(run(src).is_empty());
    }


    #[test]
    fn ignores_non_jsdoc_comment() {
        let src = "// @param {Function} cb\nfunction f(cb) {}";
        assert!(run(src).is_empty());
    }
}
