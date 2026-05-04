//! no-section-divider-comments oxc backend for TypeScript / JavaScript / TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let min_run = ctx
            .config
            .threshold("no-section-divider-comments", "min_run", ctx.lang);
        let mut diagnostics = Vec::new();
        for comment in semantic.comments() {
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            let Some(text) = ctx.source.get(start..end) else {
                continue;
            };
            if !super::is_section_divider_text(text, min_run) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, start);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Section divider comment \u{2014} signal that the file is doing \
                     too many things. Split the file by responsibility instead \
                     of decorating the boundary with `===` or `***`."
                    .into(),
                severity: Severity::Error,
                span: None,
            });
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_equals_divider() {
        assert_eq!(run("// ============").len(), 1);
    }

    #[test]
    fn flags_dashes_divider() {
        assert_eq!(run("// ----- SETUP -----").len(), 1);
    }

    #[test]
    fn flags_stars_divider() {
        assert_eq!(run("// ***** PRIVATE *****").len(), 1);
    }

    #[test]
    fn allows_short_dashes() {
        assert!(run("// -- note").is_empty());
    }

    #[test]
    fn allows_normal_comment() {
        assert!(run("// Apply the cursor advance after commit").is_empty());
    }

    #[test]
    fn ignores_dividers_in_code() {
        assert!(run("const x = '====================';").is_empty());
    }

    #[test]
    fn flags_block_comment_divider() {
        assert_eq!(run("/* ============== */").len(), 1);
    }
}
