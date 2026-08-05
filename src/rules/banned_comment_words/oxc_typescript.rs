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
            let Some((word, offset)) = super::find_banned_word(text) else {
                continue;
            };
            // Anchor on the word, not on the comment: a `/* … */` block runs
            // over as many lines as the author wrote, and the opening line
            // often holds none of what the message is about.
            let word_start = start + offset;
            let (line, column) = byte_offset_to_line_col(ctx.source, word_start);
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
                span: Some((word_start, word.len())),
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
    fn flags_reloaded() {
        assert_eq!(run("// config reloaded on each request").len(), 1);
    }

    #[test]
    fn allows_reloaded_as_prefix() {
        // word boundary: `reloadedConfig` references an identifier, not the
        // banned word — the trailing letter blocks the match.
        assert!(run("// returns reloadedConfig from the cache").is_empty());
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
    fn anchors_block_comment_on_the_word_not_the_opening_line() {
        // Same anchoring the Rust backend applies: the block opens on line 1 and
        // the word sits on line 3, which is the line the message is about.
        let src = "/* opening line, all clear here\n   second line\n   and here it is just wrong */\n";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 3);
        let word = "just";
        let (offset, len) = diags[0].span.expect("anchored on the word's byte range");
        assert_eq!(&src[offset..offset + len], word);
        let line = src.lines().nth(diags[0].line - 1).unwrap();
        assert_eq!(
            &line[diags[0].column - 1..diags[0].column - 1 + word.len()],
            word
        );
    }

    #[test]
    fn allows_deeply_and_inherently() {
        // `deeply nested` and `inherently unsafe` are legitimate technical
        // descriptions, so both words stay off the list.
        assert!(run("// deeply nested loop, inherently unsafe access").is_empty());
    }
}
