//! max-function-lines — cap every function at 30 NCLOC.
//!
//! NCLOC = Non-Commented Lines Of Code. Blank lines and lines whose
//! only content is a comment are excluded. A line that contains code
//! and a trailing comment still counts. The metric is language-
//! agnostic: the body range is provided by the TS or Rust tree-sitter
//! backend, and the shared scanner below does the counting.

mod rust;
mod typescript;

#[cfg(test)]
mod shared_tests;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "max-function-lines",
    description: "Functions longer than 30 NCLOC mix abstraction levels.",
    remediation: "Function exceeds 30 NCLOC. Extract a named helper for the \
                  tail of the body — one level of abstraction per function.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}

pub(super) const DEFAULT_MAX_LINES: usize = 30;

/// Count NCLOC in `source` between `start_row` and `end_row`
/// (0-indexed, inclusive).
///
/// Scans the whole file from the top so block comments that open
/// above `start_row` are respected. Line comments (`//`) and block
/// comments (`/* ... */`) are both recognized; TS and Rust share
/// these delimiters so one scanner serves both.
///
/// String-literal contents are not parsed, so a `/*` or `*/` inside
/// a string is treated as a comment delimiter. This produces rare
/// false negatives (multi-line raw strings carrying `/* ... */`
/// payloads), accepted in exchange for a dependency-free scanner.
pub(super) fn count_ncloc(source: &str, start_row: usize, end_row: usize) -> usize {
    let mut in_block = false;
    let mut count = 0;
    for (idx, line) in source.lines().enumerate() {
        if idx > end_row {
            break;
        }
        let has_code = line_has_code(line, &mut in_block);
        if idx >= start_row && has_code {
            count += 1;
        }
    }
    count
}

fn line_has_code(line: &str, in_block: &mut bool) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut has_code = false;
    while i < bytes.len() {
        if *in_block {
            if i + 1 < bytes.len() && bytes[i] == b'*' && bytes[i + 1] == b'/' {
                *in_block = false;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        if bytes[i].is_ascii_whitespace() {
            i += 1;
        } else if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            break;
        } else if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            *in_block = true;
            i += 2;
        } else {
            has_code = true;
            i += 1;
        }
    }
    has_code
}

#[cfg(test)]
mod ncloc_tests {
    use super::count_ncloc;

    fn ncloc(src: &str) -> usize {
        let lines = src.lines().count().saturating_sub(1);
        count_ncloc(src, 0, lines)
    }

    #[test]
    fn blank_lines_are_excluded() {
        assert_eq!(ncloc("a\n\nb\n\nc"), 3);
    }

    #[test]
    fn line_comments_are_excluded() {
        assert_eq!(ncloc("// comment\nlet x = 1;\n// trailing\nlet y = 2;"), 2);
    }

    #[test]
    fn block_comment_on_own_line_is_excluded() {
        assert_eq!(ncloc("/* hello */\nlet x = 1;"), 1);
    }

    #[test]
    fn multi_line_block_comment_is_excluded() {
        let src = "/*\n * doc\n * block\n */\nlet x = 1;";
        assert_eq!(ncloc(src), 1);
    }

    #[test]
    fn trailing_comment_does_not_drop_the_line() {
        assert_eq!(ncloc("let x = 1; // note\nlet y = 2;"), 2);
    }

    #[test]
    fn code_before_block_comment_still_counts() {
        assert_eq!(ncloc("let x = 1; /* note */\nlet y = 2;"), 2);
    }

    #[test]
    fn range_excludes_lines_outside_window() {
        let src = "one\ntwo\nthree\nfour\nfive";
        assert_eq!(count_ncloc(src, 1, 3), 3);
    }

    #[test]
    fn block_comment_opened_before_window_is_respected() {
        let src = "/*\n * header\n */\nlet x = 1;\nlet y = 2;";
        assert_eq!(count_ncloc(src, 3, 4), 2);
    }

    #[test]
    fn rustdoc_triple_slash_counts_as_line_comment() {
        assert_eq!(ncloc("/// a doc line\nfn f() {}"), 1);
    }
}
