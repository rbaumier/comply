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

            // `String.raw` cannot represent a value ending in a backslash — the
            // trailing `\` escapes the closing backtick (`String.raw`\`` is a
            // syntax error). The closing quote is a single-byte ASCII char, so
            // stripping it is boundary-safe.
            if raw[1..raw.len().saturating_sub(1)].ends_with('\\') {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_multi_backslash_string() {
        // `"\d\w+"` — two escaped backslashes, String.raw is a readability win.
        assert_eq!(run_on(r#"const re = "\\d\\w+";"#).len(), 1);
    }

    // Regression for #234: a single-backslash LIKE-escape constant needs no
    // String.raw, and String.raw can't carry a trailing backslash anyway.
    #[test]
    fn ignores_single_backslash() {
        assert!(run_on(r#"const LIKE_ESCAPE = "\\";"#).is_empty());
    }

    // Regression for #234: a value ending in a backslash cannot be a String.raw
    // literal even with 2+ escapes.
    #[test]
    fn ignores_string_ending_in_backslash() {
        assert!(run_on(r#"const x = "\\d\\";"#).is_empty());
    }
}
