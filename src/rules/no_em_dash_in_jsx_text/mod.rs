//! no-em-dash-in-jsx-text — flag em-dash (U+2014) and en-dash (U+2013) in
//! user-facing JSX copy.
//!
//! Em-dashes and en-dashes in rendered copy read as AI-generated prose; the
//! anti-slop family (`banned-comment-*`) flags the same tell in comments, this
//! one flags it in the text users actually see. It inspects `JSXText` node
//! content and the string-literal values of copy-bearing JSX attributes
//! (`title`, `label`, `placeholder`, `alt`, `aria-label`) only — never code,
//! never arbitrary expressions, never non-copy attributes. A dash flanked by
//! two digits (numeric range like `9–5`) and any dash inside `<code>` / `<pre>`
//! children are left alone.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-em-dash-in-jsx-text",
    description: "Em-dash/en-dash in user-facing JSX copy reads as AI-generated prose.",
    remediation: "Replace the em-dash (\u{2014}) or en-dash (\u{2013}) with a plain hyphen, \
                  or rewrite the sentence to drop the dash.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

/// Copy-bearing JSX attributes whose string-literal values are user-facing.
/// Anything outside this set (`className`, `id`, data-*, …) is code, not copy.
const COPY_ATTRS: &[&str] = &["title", "label", "placeholder", "alt", "aria-label"];

/// The dashes we flag: em-dash (U+2014) and en-dash (U+2013).
const DASHES: [char; 2] = ['\u{2014}', '\u{2013}'];

/// Return the byte offset of the first flagged dash in `text`, or `None`.
///
/// A dash directly between two ASCII digits (e.g. `9\u{2013}5`, `2020\u{2013}2024`)
/// is a numeric range, not prose, and is skipped.
pub(crate) fn first_dash_offset(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    for (offset, ch) in text.char_indices() {
        if !DASHES.contains(&ch) {
            continue;
        }
        if is_numeric_range(bytes, offset, ch.len_utf8()) {
            continue;
        }
        return Some(offset);
    }
    None
}

/// True when the dash at `offset` (spanning `dash_len` bytes) has an ASCII digit
/// immediately on both sides. Both neighbours are single-byte ASCII when present,
/// so the byte-level inspection never splits a multi-byte char.
fn is_numeric_range(bytes: &[u8], offset: usize, dash_len: usize) -> bool {
    let prev_is_digit = offset
        .checked_sub(1)
        .is_some_and(|i| bytes[i].is_ascii_digit());
    let next_is_digit = bytes
        .get(offset + dash_len)
        .is_some_and(u8::is_ascii_digit);
    prev_is_digit && next_is_digit
}

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_em_dash() {
        assert!(first_dash_offset("Save time \u{2014} automate.").is_some());
    }

    #[test]
    fn finds_en_dash() {
        assert!(first_dash_offset("First \u{2013} last").is_some());
    }

    #[test]
    fn ignores_plain_hyphen() {
        assert!(first_dash_offset("Save time - automate.").is_none());
    }

    #[test]
    fn skips_numeric_range_en_dash() {
        assert!(first_dash_offset("9\u{2013}5").is_none());
        assert!(first_dash_offset("2020\u{2013}2024").is_none());
    }

    #[test]
    fn skips_numeric_range_em_dash() {
        assert!(first_dash_offset("9\u{2014}5").is_none());
    }

    #[test]
    fn flags_dash_when_only_one_side_is_a_digit() {
        // "page 9 — see note" is prose, not a range.
        assert!(first_dash_offset("9 \u{2014} note").is_some());
        assert!(first_dash_offset("note \u{2014} 9").is_some());
    }

    #[test]
    fn offset_lands_on_the_dash() {
        let text = "ab\u{2014}cd";
        let off = first_dash_offset(text).unwrap();
        assert_eq!(&text[off..off + 3], "\u{2014}");
    }
}
