//! no-hex-escape OXC backend — flag `\xNN` hex escapes, prefer `\u00NN`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Locate each `\xNN` hex escape inside a literal's raw text. Returns, per
/// escape, the byte offset (relative to `text`) of the `\` that introduces the
/// escape plus the two-digit hex payload. The offset lets each diagnostic point
/// at its own escape rather than all sharing the literal's start.
fn find_hex_escapes(text: &str) -> Vec<(usize, String)> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut hits = Vec::new();

    while i + 3 < len {
        if bytes[i] == b'\\' {
            let bs_start = i;
            while i < len && bytes[i] == b'\\' {
                i += 1;
            }
            let bs_count = i - bs_start;

            if bs_count % 2 == 1
                && i < len
                && bytes[i] == b'x'
                && i + 2 < len
                && bytes[i + 1].is_ascii_hexdigit()
                && bytes[i + 2].is_ascii_hexdigit()
            {
                // `i` sits on the `x`; the unescaped backslash that pairs with
                // it is the previous byte, where the `\x` token begins.
                let escape_start = i - 1;
                let hex = &text[i + 1..i + 3];
                hits.push((escape_start, hex.to_string()));
                i += 3;
            }
        } else {
            i += 1;
        }
    }
    hits
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TemplateLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TemplateLiteral(tpl) = node.kind() else {
            return;
        };
        let raw = &ctx.source[tpl.span.start as usize..tpl.span.end as usize];
        for (offset, hex) in find_hex_escapes(raw) {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, tpl.span.start as usize + offset);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Use Unicode escape `\\u00{hex}` instead of hex escape `\\x{hex}`."
                ),
                severity: Severity::Error,
                span: None,
            });
        }
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // Also check string literals (no AstType for StringLiteral).
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let AstKind::StringLiteral(lit) = node.kind() else {
                continue;
            };
            let raw = &ctx.source[lit.span.start as usize..lit.span.end as usize];
            for (offset, hex) in find_hex_escapes(raw) {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, lit.span.start as usize + offset);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Use Unicode escape `\\u00{hex}` instead of hex escape `\\x{hex}`."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    // Regression for #5203: a literal with several `\x` escapes must report each
    // diagnostic at the column of its own escape, not all at the literal start.
    #[test]
    fn template_literal_reports_distinct_per_escape_columns() {
        // `const s = ` is 10 chars; the backtick is col 11, so the first `\x1b`
        // begins at col 12 and the second (4 chars later) at col 16.
        let diags = run("const s = `\\x1b\\x1b`;");
        let cols: Vec<usize> = diags.iter().map(|d| d.column).collect();
        assert_eq!(cols, vec![12, 16]);
        assert!(diags.iter().all(|d| d.line == 1));
    }

    // The maizzle/framework case from the issue: nine `\x1b` escapes in one
    // template literal must report nine distinct columns, each at its own `\x`.
    #[test]
    fn many_escapes_report_distinct_columns() {
        let diags = run(
            "info(`  \\x1b[32m\\x1b[1mMAIZZLE\\x1b[0m\\x1b[32m v\\x1b[0m  \\x1b[2m\\x1b[0m \\x1b[1m\\x1b[0m`)",
        );
        assert_eq!(diags.len(), 9);
        let cols: Vec<usize> = diags.iter().map(|d| d.column).collect();
        // Distinct and strictly increasing — no two share the literal start.
        for w in cols.windows(2) {
            assert!(w[0] < w[1], "columns must be increasing: {cols:?}");
        }
    }

    #[test]
    fn single_escape_reports_its_own_column() {
        let diags = run("const s = `\\x41`;");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].column, 12);
        assert_eq!(diags[0].line, 1);
    }

    // The escape's column is resolved on its own line, not line 1.
    #[test]
    fn escape_column_is_line_relative() {
        let diags = run("const a = 1;\nconst s = `\\x1b`;");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 2);
        assert_eq!(diags[0].column, 12);
    }

    #[test]
    fn no_escape_no_diagnostic() {
        assert!(run("const s = `plain`;").is_empty());
    }
}
