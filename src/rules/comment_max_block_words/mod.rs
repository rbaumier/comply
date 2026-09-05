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
    id: "comment-max-block-words",
    description: "Comment block exceeds the configured word budget.",
    remediation: "Trim the block — past the budget a comment stops being read, doc comment or not.",
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
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Sql, Backend::Text(Box::new(sql_text::Check))),
        ],
    }
}

/// A block over budget: where it starts and how many words it holds.
pub(crate) struct Flag {
    pub line: usize,
    pub column: usize,
    pub words: usize,
}

/// Flag every merged block holding more than `max` words.
/// License banners are exempt: their length is fixed by the license.
pub(crate) fn flagged_blocks(comments: Vec<RawComment>, source: &str, max: usize) -> Vec<Flag> {
    comment_blocks::merge(comments, source)
        .into_iter()
        .filter(|block| block.exceeds_budget(max))
        .map(|block| Flag { line: block.line, column: block.column, words: block.word_count() })
        .collect()
}

/// The diagnostic message for a block of `words` under budget `max`.
pub(crate) fn message(words: usize, max: usize) -> String {
    format!("Comment block spans {words} words (max {max}). Split it or cut the detail.")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comment(start_byte: usize, line: usize, raw: &str) -> RawComment {
        RawComment { start_byte, line, column: 1, raw: raw.into(), is_line: true }
    }

    #[test]
    fn merges_consecutive_line_comments() {
        let comments = vec![comment(0, 1, "// a b c"), comment(9, 2, "// d e f")];
        let source = comment_blocks::source_of(&comments);
        let flags = flagged_blocks(comments, &source, 5);
        assert_eq!(flags.len(), 1);
        assert_eq!(flags[0].line, 1);
        assert_eq!(flags[0].words, 6);
    }

    #[test]
    fn a_blank_line_gap_shares_one_budget() {
        let comments = vec![comment(0, 1, "// a b c"), comment(9, 3, "// d e f")];
        let source = comment_blocks::source_of(&comments);
        assert_eq!(flagged_blocks(comments, &source, 5).len(), 1);
    }

    #[test]
    fn a_line_of_code_starts_a_new_budget() {
        let comments = vec![comment(0, 1, "// a b c"), comment(20, 3, "// d e f")];
        assert!(flagged_blocks(comments, "// a b c\nlet x = 1;\n// d e f", 5).is_empty());
    }

    #[test]
    fn doc_comments_count_too() {
        let comments = vec![comment(0, 1, "/// one two three four five six")];
        let source = comment_blocks::source_of(&comments);
        assert_eq!(flagged_blocks(comments, &source, 3).len(), 1);
    }

    #[test]
    fn license_banner_is_exempt() {
        let comments =
            vec![comment(0, 1, "// Copyright 2026 the authors under the terms of the license")];
        let source = comment_blocks::source_of(&comments);
        assert!(flagged_blocks(comments, &source, 3).is_empty());
    }
}
