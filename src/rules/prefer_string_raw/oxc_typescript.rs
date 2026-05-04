use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Count `\\` pairs in a string node's source text.
fn count_escaped_backslashes(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut count = 0;
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'\\' && bytes[i + 1] == b'\\' {
            count += 1;
            i += 2;
        } else {
            i += 1;
        }
    }
    count
}

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let AstKind::StringLiteral(lit) = node.kind() else { continue };
            let raw = &ctx.source[lit.span.start as usize..lit.span.end as usize];

            // Skip strings containing backticks (can't use String.raw with backticks)
            if raw.contains('`') {
                continue;
            }

            // Skip strings with interpolation patterns
            if raw.contains("${") {
                continue;
            }

            if count_escaped_backslashes(raw) >= 2 {
                let (line, column) = byte_offset_to_line_col(ctx.source, lit.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`String.raw` should be used to avoid escaping `\\`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}
