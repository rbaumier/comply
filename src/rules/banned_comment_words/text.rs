//! banned-comment-words backend — scan comment lines for dismissive filler.
//!
//! A match must be inside a comment, so the line is read from its `//` or `/*`
//! marker on. The word list, the word-boundary test and the sense tests are
//! `super::find_banned_word`'s, so a Vue file gets the verdict a `.ts` file
//! with the same comment gets.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::comment_blocks::RawComment;

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let comments = comment_lines(ctx.source);
        let matches: Vec<(usize, &'static str)> = comments
            .iter()
            .filter_map(|comment| {
                super::find_banned_word(&comment.raw).map(|(word, _)| (comment.line, word))
            })
            .collect();
        let budget = super::explanation_budget(ctx);
        let explained = super::explained_rows(comments, ctx.source, budget);
        matches
            .into_iter()
            .filter(|(line, _)| !explained.contains(line))
            .map(|(line, word)| Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "Comment uses `{word}` — dismissive filler that hides complexity. \
                     Either explain the actual subtlety or delete the comment if the \
                     line is genuinely self-explanatory."
                ),
                severity: Severity::Error,
                span: None,
            })
            .collect()
    }
}

/// Every line holding a comment, from its marker on, as the block merger reads
/// comments. One line is one comment here: a `/* … */` spanning several lines
/// is scanned line by line, which is the unit this backend reports on.
fn comment_lines(source: &str) -> Vec<RawComment> {
    let mut comments = Vec::new();
    let mut byte = 0;
    for (offset, line) in source.lines().enumerate() {
        if let Some(column) = line.find("//").or_else(|| line.find("/*")) {
            let raw = &line[column..];
            comments.push(RawComment {
                start_byte: byte + column,
                line: offset + 1,
                column: column + 1,
                raw: raw.to_string(),
                is_line: raw.starts_with("//"),
            });
        }
        byte += line.len() + 1;
    }
    comments
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
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
        // word boundary: `simplify` contains `simply` only as a prefix-ish
        // substring; word boundary check rejects it.
        assert!(run("// We simplify the input").is_empty());
    }

    #[test]
    fn allows_understanding() {
        // No banned word inside.
        assert!(run("// understanding the data flow").is_empty());
    }

    #[test]
    fn ignores_banned_word_in_code() {
        // Outside a comment, the rule must not fire.
        assert!(run("const obviously = true;").is_empty());
    }

    #[test]
    fn one_diagnostic_per_line() {
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
    fn flags_importantly() {
        assert_eq!(run("// importantly, the order matters here").len(), 1);
    }

    #[test]
    fn allows_actually() {
        assert!(run("// actually resolved at build time").is_empty());
    }

    #[test]
    fn allows_negated_simply() {
        // "not simply" reverses the dismissive import — explanatory, not filler.
        assert!(run("// this will not simply filter the entries").is_empty());
    }

    #[test]
    fn allows_the_non_hedge_senses_of_just_issue_8310() {
        // The same comments the `.rs` and `.ts` backends read, and the same
        // verdicts: the sense tests live in the scanner all three share.
        let src = "\
// ripgrep must sniff utf-8 BOM, just like it does with utf-16.
const a = 1;

// We have just enough space.
const b = 2;

// Get a mutable view into the bytes we've just read.
const c = 3;

// If the entire glob is just `**`, then it should match everything.
const d = 4;

// just call foo and it works
const e = 5;
";
        let diags = run(src);
        assert_eq!(diags.iter().map(|d| d.line).collect::<Vec<_>>(), vec![13]);
    }

    #[test]
    fn allows_negation_following_the_filler_word_issue_8184() {
        assert!(run("// I really can't see a better way").is_empty());
    }

    #[test]
    fn allows_banned_word_in_a_block_over_the_word_budget_issue_8184() {
        let src = "\
// The reader needs the whole story here, so this note names the compiler
// version, the shorter form that fails to build, what the checker infers
// instead, why that inference is wrong for this call, and it ends on the
// upstream issue, because we could just write the shorter form otherwise.";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_dismissive_word_after_negated_one() {
        // The negated "simply" is skipped, but the later un-negated "just"
        // is still caught.
        assert_eq!(run("// not simply, just call foo").len(), 1);
    }
}
