mod oxc_typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::comment_blocks::{self, RawComment};
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "comment-no-mid-sentence-wrap",
    description: "A comment line breaks mid-sentence.",
    remediation: "Break the line on punctuation, or shorten the sentence so it fits on one line.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["comments"],

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

pub(crate) const MESSAGE: &str =
    "Comment line breaks mid-sentence. End the line on punctuation, or shorten it.";

/// The first line of a block whose sentence runs on.
pub(crate) struct Flag {
    pub line: usize,
    pub column: usize,
}

/// Flag the first wrapped sentence of every block.
/// One flag per block keeps a rewrapped paragraph from firing on every line.
/// License banners are exempt: their line breaks come with the license.
pub(crate) fn flagged_wraps(comments: Vec<RawComment>) -> Vec<Flag> {
    let mut flags = Vec::new();
    for block in comment_blocks::merge(comments) {
        if block.is_license() {
            continue;
        }
        let wrapped = block
            .lines
            .windows(2)
            .find(|pair| wraps(&pair[0].text, &pair[1].text));
        if let Some(pair) = wrapped {
            flags.push(Flag {
                line: pair[0].line,
                column: block.column,
            });
        }
    }
    flags
}

/// True when `current` breaks somewhere a reader cannot pause.
/// A line may end on punctuation.
/// Ending on a bare word means the sentence runs on.
fn wraps(current: &str, next: &str) -> bool {
    let both_carry_prose = !current.is_empty() && !next.is_empty();
    if !both_carry_prose || closes_line(current) {
        return false;
    }
    !opens_structure(next)
}

/// True when a reader can stop at the end of `line`.
/// Any punctuation closes it, and so does a markdown line that stands alone.
fn closes_line(line: &str) -> bool {
    let tail = line.trim_end_matches([')', ']', '"', '\'', '`', '*', '_']);
    tail.ends_with(['.', '!', '?', ':', ';', ',', '—', '–'])
        || opens_structure(line)
        || (line.starts_with('[') && line.contains("]:"))
}

/// True when `line` opens markdown structure rather than continuing prose.
fn opens_structure(line: &str) -> bool {
    line.starts_with(['#', '|', '>'])
        || ["- ", "* ", "+ "]
            .iter()
            .any(|bullet| line.starts_with(bullet))
        || line
            .split_once(['.', ')'])
            .is_some_and(|(head, _)| !head.is_empty() && head.chars().all(|c| c.is_ascii_digit()))
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
    fn flags_a_sentence_running_onto_the_next_line() {
        let comments = vec![
            comment(0, 1, "/// Non-stream completions POST through"),
            comment(40, 2, "/// `reqwest`; the stream leg builds one client."),
        ];
        let flags = flagged_wraps(comments);
        assert_eq!(flags.len(), 1);
        assert_eq!(flags[0].line, 1);
    }

    #[test]
    fn allows_one_sentence_per_line() {
        let comments = vec![
            comment(0, 1, "/// Holds the connection settings."),
            comment(35, 2, "/// Each call builds its own client."),
        ];
        assert!(flagged_wraps(comments).is_empty());
    }

    #[test]
    fn allows_a_line_broken_after_a_comma() {
        let comments = vec![
            comment(0, 1, "// The pool times out under load,"),
            comment(34, 2, "// so the retry loop hides the cause."),
        ];
        assert!(flagged_wraps(comments).is_empty());
    }

    #[test]
    fn allows_a_line_broken_after_a_semicolon() {
        let comments = vec![
            comment(0, 1, "/// Non-stream completions POST through reqwest;"),
            comment(48, 2, "/// the stream leg builds one client per attempt."),
        ];
        assert!(flagged_wraps(comments).is_empty());
    }

    #[test]
    fn flags_notes_stacked_without_punctuation() {
        let comments = vec![
            comment(0, 1, "// why: the pool times out under load"),
            comment(38, 2, "// gotcha: the retry loop hides the cause"),
        ];
        assert_eq!(flagged_wraps(comments).len(), 1);
    }

    #[test]
    fn a_colon_may_introduce_the_next_line() {
        let comments = vec![
            comment(0, 1, "// Two failure modes matter here:"),
            comment(34, 2, "// - the endpoint is missing"),
            comment(63, 3, "// - the endpoint is malformed"),
        ];
        assert!(flagged_wraps(comments).is_empty());
    }

    #[test]
    fn markdown_headings_do_not_wrap() {
        let comments = vec![
            comment(0, 1, "/// # Errors"),
            comment(
                13,
                2,
                "/// Returns an error when the endpoint is unreachable.",
            ),
        ];
        assert!(flagged_wraps(comments).is_empty());
    }

    #[test]
    fn one_flag_per_block() {
        let comments = vec![
            comment(
                0,
                1,
                "// The client opens a fresh connection per request and only",
            ),
            comment(
                60,
                2,
                "// needs a mutable borrow to stash the response headers it",
            ),
            comment(119, 3, "// just read from the wire."),
        ];
        assert_eq!(flagged_wraps(comments).len(), 1);
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
        assert!(flagged_wraps(comments).is_empty());
    }
}
