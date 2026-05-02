//! sql-index-needs-rationale-comment — oxc backend for TS / JS / TSX.
//!
//! Walks StringLiteral and TemplateLiteral nodes, strips delimiters, and
//! delegates to `rust::check_string_content` for the actual detection.

use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};

use super::rust::check_string_content;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StringLiteral, AstType::TemplateLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let min_rationale_words = ctx
            .config
            .threshold("sql-index-needs-rationale-comment", "min_rationale_words", ctx.lang);
        let lookback_lines = ctx
            .config
            .threshold("sql-index-needs-rationale-comment", "lookback_lines", ctx.lang);

        let (content, offset) = match node.kind() {
            AstKind::StringLiteral(lit) => {
                (lit.value.as_str(), lit.span.start as usize)
            }
            AstKind::TemplateLiteral(tpl) => {
                // For template literals with multiple quasis, join them.
                // We only use the first quasi's raw text if there's one segment;
                // otherwise fall back to full raw text from source.
                if tpl.quasis.len() == 1 {
                    (tpl.quasis[0].value.raw.as_str(), tpl.span.start as usize)
                } else {
                    // Multi-segment: extract from source between backticks.
                    let start = tpl.span.start as usize + 1;
                    let end = tpl.span.end as usize;
                    let end = if end > 0 { end - 1 } else { end };
                    if let Some(slice) = ctx.source.get(start..end) {
                        (slice, tpl.span.start as usize)
                    } else {
                        return;
                    }
                }
            }
            _ => return,
        };

        let (line, col) = byte_offset_to_line_col(ctx.source, offset);
        // line is 1-based from byte_offset_to_line_col, but check_string_content
        // expects a 0-based row index (it adds +1 internally).
        diagnostics.extend(check_string_content(
            content,
            line.saturating_sub(1),
            col,
            ctx.path,
            min_rationale_words,
            lookback_lines,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_create_index_without_comment() {
        let src = "const sql = `CREATE INDEX idx_foo ON bar(baz);`;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_create_index_with_preceding_comment() {
        let src = "const sql = `-- Accelerates dashboard query for user_id\nCREATE INDEX idx_foo ON bar(baz);`;";
        assert!(run_on(src).is_empty());
    }
}
