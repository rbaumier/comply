mod oxc_typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "comment-max-block-words",
    description: "Comment block exceeds the configured word budget.",
    remediation: "Trim the block or move the long-form explanation into a doc comment — an oversized inline block stops being read.",
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
        ],
    }
}

/// One comment token as reported by a backend, with the position data the
/// block merger needs. `line`/`column` are 1-based (the diagnostic anchor and
/// the grouping key); `start_byte` orders tokens in source order.
pub(crate) struct RawComment {
    pub start_byte: usize,
    pub line: usize,
    pub column: usize,
    pub raw: String,
    pub is_line: bool,
}

/// A flagged block: where to anchor the diagnostic and how many words it holds.
pub(crate) struct Flag {
    pub line: usize,
    pub column: usize,
    pub words: usize,
}

/// Documentation comments are exempt: `///` / `//!` outer-and-inner Rust docs
/// and `/**` / `/*!` doc blocks are API prose where full explanations are
/// expected. The budget targets *implementation-note* bloat, not docs.
fn is_doc_comment(raw: &str) -> bool {
    let t = raw.trim_start();
    t.starts_with("///") || t.starts_with("//!") || t.starts_with("/**") || t.starts_with("/*!")
}

/// License / copyright banners are duplicated by design and cannot be shortened,
/// so their length is not a smell.
fn is_license_block(lower: &str) -> bool {
    const MARKERS: &[&str] = &[
        "copyright",
        "spdx-license-identifier",
        "licensed under",
        "all rights reserved",
        "@license",
        "@copyright",
    ];
    MARKERS.iter().any(|m| lower.contains(m))
}

/// Strip the comment markers off each line of `raw` and count the whitespace-
/// separated words that remain.
fn count_words(raw: &str) -> usize {
    raw.lines()
        .map(|line| {
            line.trim()
                .trim_start_matches("//")
                .trim_start_matches("/*")
                .trim_start_matches("*/")
                .trim_start_matches('*')
                .trim_end_matches("*/")
                .split_whitespace()
                .count()
        })
        .sum()
}

/// Merge consecutive `//` lines (same indent, no row gap) into one logical
/// block, treat each `/* */` node as its own block, and flag every block whose
/// total word count exceeds `max`. Doc comments and license banners are exempt.
pub(crate) fn flagged_blocks(mut comments: Vec<RawComment>, max: usize) -> Vec<Flag> {
    comments.retain(|c| !is_doc_comment(&c.raw));
    comments.sort_by_key(|c| c.start_byte);

    let mut flags = Vec::new();
    let mut i = 0;
    while i < comments.len() {
        let start = i;
        let anchor_line = comments[i].line;
        let anchor_col = comments[i].column;
        i += 1;
        if comments[start].is_line {
            while i < comments.len()
                && comments[i].is_line
                && comments[i].column == anchor_col
                && comments[i].line == comments[i - 1].line + 1
            {
                i += 1;
            }
        }

        let mut words = 0;
        let mut lower = String::new();
        for c in &comments[start..i] {
            words += count_words(&c.raw);
            lower.push_str(&c.raw.to_lowercase());
            lower.push('\n');
        }
        if words > max && !is_license_block(&lower) {
            flags.push(Flag { line: anchor_line, column: anchor_col, words });
        }
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_consecutive_line_comments() {
        let comments = vec![
            RawComment { start_byte: 0, line: 1, column: 1, raw: "// a b c".into(), is_line: true },
            RawComment { start_byte: 9, line: 2, column: 1, raw: "// d e f".into(), is_line: true },
        ];
        // 6 words total, cap 5 -> one flag anchored on line 1.
        let flags = flagged_blocks(comments, 5);
        assert_eq!(flags.len(), 1);
        assert_eq!(flags[0].line, 1);
        assert_eq!(flags[0].words, 6);
    }

    #[test]
    fn blank_line_gap_splits_blocks() {
        let comments = vec![
            RawComment { start_byte: 0, line: 1, column: 1, raw: "// a b c".into(), is_line: true },
            RawComment { start_byte: 9, line: 3, column: 1, raw: "// d e f".into(), is_line: true },
        ];
        assert!(flagged_blocks(comments, 5).is_empty());
    }

    #[test]
    fn doc_comments_are_exempt() {
        let comments = vec![RawComment {
            start_byte: 0,
            line: 1,
            column: 1,
            raw: "/// one two three four five six".into(),
            is_line: true,
        }];
        assert!(flagged_blocks(comments, 3).is_empty());
    }

    #[test]
    fn license_banner_is_exempt() {
        let comments = vec![RawComment {
            start_byte: 0,
            line: 1,
            column: 1,
            raw: "// Copyright 2026 the authors under the terms of the license agreement here".into(),
            is_line: true,
        }];
        assert!(flagged_blocks(comments, 3).is_empty());
    }
}
