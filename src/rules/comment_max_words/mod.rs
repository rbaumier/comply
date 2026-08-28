mod oxc_typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::comment_blocks::{self, RawComment};
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "comment-max-words",
    description: "Comment sentence exceeds the configured word budget.",
    remediation: "Split long comment sentences — one idea per sentence keeps the intent scannable.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}

/// A sentence over budget: where it starts and how long it runs.
pub(crate) struct Flag {
    pub line: usize,
    pub column: usize,
    pub words: usize,
}

/// Flag every sentence longer than `max` words.
/// Sentences are counted across the whole block, not line by line.
/// License banners are exempt: their wording is fixed by the license.
pub(crate) fn flagged_sentences(comments: Vec<RawComment>, max: usize) -> Vec<Flag> {
    let mut flags = Vec::new();
    for block in comment_blocks::merge(comments) {
        if block.is_license() {
            continue;
        }
        flags.extend(over_budget_sentences(&block, max));
    }
    flags
}

/// Walk `block` word by word and flag each sentence past `max`.
fn over_budget_sentences(block: &comment_blocks::CommentBlock, max: usize) -> Vec<Flag> {
    let mut flags = Vec::new();
    let mut start_line = block.line;
    let mut words = 0;
    for (line, token) in block.tokens() {
        if words == 0 {
            start_line = line;
        }
        words += 1;
        if !ends_sentence(token) {
            continue;
        }
        if words > max {
            flags.push(Flag {
                line: start_line,
                column: block.column,
                words,
            });
        }
        words = 0;
    }
    if words > max {
        flags.push(Flag {
            line: start_line,
            column: block.column,
            words,
        });
    }
    flags
}

/// The diagnostic message for a sentence of `words` under budget `max`.
pub(crate) fn message(words: usize, max: usize) -> String {
    format!("Comment sentence runs {words} words (max {max}). Split it — one idea per sentence.")
}

/// True when `token` closes a sentence.
/// Trailing brackets and quotes sit outside the punctuation.
fn ends_sentence(token: &str) -> bool {
    token
        .trim_end_matches([')', ']', '"', '\'', '`'])
        .ends_with(['.', '!', '?'])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comment(start_byte: usize, line: usize, raw: &str) -> RawComment {
        RawComment {
            start_byte,
            line,
            column: 1,
            raw: raw.into(),
            is_line: true,
        }
    }

    #[test]
    fn counts_a_sentence_across_wrapped_lines() {
        let comments = vec![
            comment(0, 1, "// one two three four"),
            comment(22, 2, "// five six seven eight."),
        ];
        let flags = flagged_sentences(comments, 5);
        assert_eq!(flags.len(), 1);
        assert_eq!(flags[0].words, 8);
        assert_eq!(flags[0].line, 1);
    }

    #[test]
    fn anchors_on_the_line_the_sentence_starts() {
        let comments = vec![
            comment(0, 1, "// Short one."),
            comment(14, 2, "// one two three four five six seven."),
        ];
        let flags = flagged_sentences(comments, 5);
        assert_eq!(flags.len(), 1);
        assert_eq!(flags[0].line, 2);
    }

    #[test]
    fn short_sentences_pass() {
        let comments = vec![
            comment(0, 1, "// One idea here."),
            comment(18, 2, "// Another idea here."),
        ];
        assert!(flagged_sentences(comments, 5).is_empty());
    }

    #[test]
    fn license_banner_is_exempt() {
        let comments = vec![
            comment(
                0,
                1,
                "// Copyright 2026 the authors, licensed under the terms",
            ),
            comment(56, 2, "// of the agreement shipped alongside this file."),
        ];
        assert!(flagged_sentences(comments, 5).is_empty());
    }

    #[test]
    fn identifier_dots_do_not_close_a_sentence() {
        let comments = vec![comment(
            0,
            1,
            "// ctx.config holds one two three four five six.",
        )];
        let flags = flagged_sentences(comments, 5);
        assert_eq!(flags.len(), 1);
        assert_eq!(flags[0].words, 8);
    }
}
