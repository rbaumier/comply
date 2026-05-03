//! jsdoc-reject-any-type OXC backend — flag `{*}` / `{any}` in JSDoc comments.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn find_any_types_in_line(line: &str) -> Vec<usize> {
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
                if type_content == "*" || type_content.eq_ignore_ascii_case("any") {
                    hits.push(start);
                }
            }
        }
        i += 1;
    }
    hits
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
            // OXC comment spans exclude the `//` or `/*` prefix markers.
            // We need to check raw source to see if this is a `/**` JSDoc comment.
            // The `/*` starts 2 bytes before span.start.
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

            // Compute byte offset of the `/**` start for line/col.
            let (base_line, _) = byte_offset_to_line_col(ctx.source, doc_start);

            for (line_idx, line) in raw.lines().enumerate() {
                for col in find_any_types_in_line(line) {
                    let abs_line = base_line + line_idx;
                    let abs_col = if line_idx == 0 {
                        let (_, base_col) = byte_offset_to_line_col(ctx.source, doc_start);
                        base_col + col
                    } else {
                        col + 1
                    };
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line: abs_line,
                        column: abs_col,
                        rule_id: super::META.id.into(),
                        message: "JSDoc uses `*` or `any` type \u{2014} provide a specific type instead.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
        diagnostics
    }
}
