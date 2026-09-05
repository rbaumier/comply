//! banned-comment-words oxc backend for TypeScript / JavaScript / TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::comment_blocks;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let budget = super::explanation_budget(ctx);
        let explained = super::explained_rows(
            comment_blocks::from_oxc(semantic, ctx.source),
            ctx.source,
            budget,
        );
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
            if explained.contains(&line) {
                continue;
            }
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
        let src = "/* opening line, all clear here\n   second line\n   and here it goes just wrong */\n";
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

    /// The `kpdecker/jsdiff` comment reported in #8184, verbatim: it names the
    /// compiler version, the shorter form that fails, what the checker infers
    /// instead, and the upstream issue. `comment-max-block-words` reports the
    /// same block for its length.
    const JSDIFF_BLOCK: &str = "\
// It would be cleaner if instead of the line below we could just write
//     return patch.map(unixToWin)
// but mysteriously TypeScript (v5.7.3 at the time of writing) does not like this and it will
// refuse to compile, thinking that unixToWin could then return StructuredPatch[][] and the
// result would be incompatible with the overload signatures.
// See bug report at https://github.com/microsoft/TypeScript/issues/61398.
export const c = 3;
";

    #[test]
    fn allows_banned_word_in_a_block_over_the_word_budget_issue_8184() {
        assert!(run(JSDIFF_BLOCK).is_empty());
    }

    #[test]
    fn flags_the_same_sentence_on_its_own() {
        // The gate is the block's length, not the sentence's wording.
        assert_eq!(run("// we could just write the shorter form").len(), 1);
    }

    /// The four senses of `just` reported in #8310, one per comment, closed by
    /// the hedge the rule targets. Every backend has to read them the same way.
    const JUST_SENSES: &str = "\
// ripgrep must sniff utf-8 BOM, just like it does with utf-16.
export const a = 1;

// We have just enough space.
export const b = 2;

// Get a mutable view into the bytes we've just read.
export const c = 3;

// If the entire glob is just `**`, then it should match everything.
export const d = 4;

// just call foo and it works
export const e = 5;
";

    #[test]
    fn allows_the_non_hedge_senses_of_just_issue_8310() {
        let diags = run(JUST_SENSES);
        assert_eq!(diags.iter().map(|d| d.line).collect::<Vec<_>>(), vec![13]);
    }

    #[test]
    fn allows_negation_on_either_side_of_the_filler_word_issue_8184() {
        assert!(run("// I can't really see a better way").is_empty());
        assert!(run("// I really can't see a better way").is_empty());
    }

    #[test]
    fn allows_deeply_and_inherently() {
        // `deeply nested` and `inherently unsafe` are legitimate technical
        // descriptions, so both words stay off the list.
        assert!(run("// deeply nested loop, inherently unsafe access").is_empty());
    }
}
