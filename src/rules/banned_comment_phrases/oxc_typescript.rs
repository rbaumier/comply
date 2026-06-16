//! banned-comment-phrases oxc backend for TypeScript / JavaScript / TSX.

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
        let mut diagnostics = Vec::new();
        for comment in semantic.comments() {
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            let Some(text) = ctx.source.get(start..end) else {
                continue;
            };
            let Some(phrase) = super::find_banned_phrase(text) else {
                continue;
            };
            let (line, column) = byte_offset_to_line_col(ctx.source, start);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Comment uses `{phrase}` \u{2014} narrator filler typical of \
                     AI-generated prose. State the point directly or delete the comment."
                ),
                severity: Severity::Warning,
                span: None,
            });
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
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_heres_what() {
        assert_eq!(run("// here's what the cache does").len(), 1);
    }

    #[test]
    fn flags_deep_dive() {
        assert_eq!(run("// deep dive on the retry logic below").len(), 1);
    }

    #[test]
    fn flags_block_comment() {
        assert_eq!(run("/* let me walk you through the flow */").len(), 1);
    }

    #[test]
    fn allows_clean_comment() {
        assert!(run("// retries twice on a 503, then gives up").is_empty());
    }

    #[test]
    fn ignores_phrase_in_string_literal() {
        // The rule scans comments only — a phrase in code stays untouched.
        assert!(run("const label = \"deep dive\";").is_empty());
    }

    #[test]
    fn boundary_blocks_deepest_dive() {
        // `deep dive` must be contiguous; "deepest dive" is not a match.
        assert!(run("// the deepest dive into the parser yet").is_empty());
    }
}
