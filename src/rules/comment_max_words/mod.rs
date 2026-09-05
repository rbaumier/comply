mod oxc_typescript;
mod rust;
mod sql_text;

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
            (Language::Sql, Backend::Text(Box::new(sql_text::Check))),
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
pub(crate) fn flagged_sentences(comments: Vec<RawComment>, source: &str, max: usize) -> Vec<Flag> {
    let mut flags = Vec::new();
    for block in comment_blocks::merge(comments, source) {
        if block.is_license() {
            continue;
        }
        flags.extend(over_budget_sentences(&block, max));
    }
    flags
}

/// The sentence being read: the row it opened on and its word count.
#[derive(Clone, Copy)]
struct Sentence {
    line: usize,
    words: usize,
}

impl Sentence {
    /// The finished sentence, reported when it ran past `max` words.
    fn over_budget(self, column: usize, max: usize) -> Option<Flag> {
        (self.words > max).then_some(Flag {
            line: self.line,
            column,
            words: self.words,
        })
    }
}

/// Walk the prose of `block` word by word and flag each sentence past `max`.
///
/// A line that is not prose closes the sentence it interrupts: a banner, a
/// tool directive and a fenced sample are read on their own, so their words
/// never spend the budget of the prose next to them.
fn over_budget_sentences(block: &comment_blocks::CommentBlock, max: usize) -> Vec<Flag> {
    let mut flags = Vec::new();
    let mut sentence = Sentence {
        line: block.line,
        words: 0,
    };
    for line in &block.lines {
        if line.kind != comment_blocks::LineKind::Prose {
            flags.extend(sentence.over_budget(block.column, max));
            sentence = Sentence {
                line: line.line,
                words: 0,
            };
            continue;
        }
        for token in line.text.split_whitespace() {
            if comment_blocks::is_word(token) {
                if sentence.words == 0 {
                    sentence.line = line.line;
                }
                sentence.words += 1;
            }
            if ends_sentence(token) {
                flags.extend(sentence.over_budget(block.column, max));
                sentence = Sentence {
                    line: line.line,
                    words: 0,
                };
            }
        }
    }
    flags.extend(sentence.over_budget(block.column, max));
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
        let source = comment_blocks::source_of(&comments);
        let flags = flagged_sentences(comments, &source, 5);
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
        let source = comment_blocks::source_of(&comments);
        let flags = flagged_sentences(comments, &source, 5);
        assert_eq!(flags.len(), 1);
        assert_eq!(flags[0].line, 2);
    }

    #[test]
    fn short_sentences_pass() {
        let comments = vec![
            comment(0, 1, "// One idea here."),
            comment(18, 2, "// Another idea here."),
        ];
        let source = comment_blocks::source_of(&comments);
        assert!(flagged_sentences(comments, &source, 5).is_empty());
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
        let source = comment_blocks::source_of(&comments);
        assert!(flagged_sentences(comments, &source, 5).is_empty());
    }

    #[test]
    fn identifier_dots_do_not_close_a_sentence() {
        let comments = vec![comment(
            0,
            1,
            "// ctx.config holds one two three four five six.",
        )];
        let source = comment_blocks::source_of(&comments);
        let flags = flagged_sentences(comments, &source, 5);
        assert_eq!(flags.len(), 1);
        assert_eq!(flags[0].words, 8);
    }
}
