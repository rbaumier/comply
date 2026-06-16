//! banned-comment-words oxc backend for TypeScript / JavaScript / TSX.

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
            let Some(word) = super::find_banned_word(text) else {
                continue;
            };
            let (line, column) = byte_offset_to_line_col(ctx.source, start);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Comment uses `{word}` \u{2014} dismissive filler that hides complexity. \
                     Either explain the actual subtlety or delete the comment if the \
                     line is genuinely self-explanatory."
                ),
                severity: Severity::Error,
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
    fn flags_simply() {
        assert_eq!(run("// This simply works").len(), 1);
    }

    #[test]
    fn flags_obviously() {
        assert_eq!(run("// Obviously the cache wins").len(), 1);
    }

    #[test]
    fn flags_just() {
        assert_eq!(run("// just retry on failure").len(), 1);
    }

    #[test]
    fn allows_simplify() {
        assert!(run("// We simplify the input").is_empty());
    }

    #[test]
    fn allows_understanding() {
        assert!(run("// understanding the data flow").is_empty());
    }

    #[test]
    fn ignores_banned_word_in_code() {
        assert!(run("const obviously = true;").is_empty());
    }

    #[test]
    fn one_diagnostic_per_comment() {
        assert_eq!(run("// just simply works").len(), 1);
    }

    #[test]
    fn flags_block_comment() {
        assert_eq!(run("/* this is basically wrong */").len(), 1);
    }

    #[test]
    fn flags_updated() {
        assert_eq!(run("// updated to use the new API").len(), 1);
    }

    #[test]
    fn flags_reloaded() {
        assert_eq!(run("// config reloaded on each request").len(), 1);
    }

    #[test]
    fn allows_updated_as_prefix() {
        // word boundary: `updatedProduct` references an identifier, not the
        // banned word — the trailing letter blocks the match.
        assert!(run("// returns updatedProduct from the cache").is_empty());
    }

    #[test]
    fn flags_crucially() {
        assert_eq!(run("// crucially, this must run before flush").len(), 1);
    }

    #[test]
    fn flags_really() {
        assert_eq!(run("// really only needed on the cold path").len(), 1);
    }

    #[test]
    fn allows_actually() {
        // `actually` is excluded: in code it commonly contrasts expectation
        // with reality (`actually computed lazily`) — too many false positives.
        assert!(run("// the value is actually computed lazily").is_empty());
    }

    #[test]
    fn allows_deeply_and_inherently() {
        // `deeply nested` and `inherently unsafe` are legitimate technical
        // descriptions, so both words stay off the list.
        assert!(run("// deeply nested loop, inherently unsafe access").is_empty());
    }
}
