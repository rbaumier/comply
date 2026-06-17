//! Shared scanner helpers for regex-pattern lint rules (oxc backend).
//!
//! Regex rules scan the extracted pattern byte-by-byte to detect antipatterns.
//! Several of them need the same character-class context: inside `[...]`, the
//! characters `(`, `)`, `{`, `}`, `*`, `?`, `+`, `^`, `$` are literal members,
//! not syntax. This module owns that one assumption so each rule shares one
//! correct implementation instead of re-deriving (and mis-deriving) it.

/// Returns true if the byte at `target` is inside a `[...]` character class.
/// Tracks `[` / `]` while respecting `\` escapes and the JavaScript/POSIX rule
/// that the first `]` after `[` (or `[^`) is a literal character, not a closer.
#[must_use]
pub fn is_inside_char_class(bytes: &[u8], target: usize) -> bool {
    let mut inside = false;
    // When `just_opened` is true, the next `]` is treated as a literal.
    let mut just_opened = false;
    let mut i = 0;
    while i < target {
        match bytes[i] {
            b'\\' => {
                // Guard against a trailing backslash to avoid OOB panic.
                i = i.saturating_add(2).min(bytes.len());
                just_opened = false;
            }
            b'[' if !inside => {
                inside = true;
                just_opened = true;
                i += 1;
                // Skip optional `^` at the start of a negated class.
                if i < target && i < bytes.len() && bytes[i] == b'^' {
                    i += 1;
                }
            }
            b']' if inside => {
                if just_opened {
                    // First `]` after `[` or `[^` is a literal in JS regex.
                    just_opened = false;
                    i += 1;
                } else {
                    inside = false;
                    i += 1;
                }
            }
            _ => {
                just_opened = false;
                i += 1;
            }
        }
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outside_class_is_false() {
        let b = b"abc(){}";
        for i in 0..b.len() {
            assert!(!is_inside_char_class(b, i), "index {i} should be outside a class");
        }
    }

    #[test]
    fn content_inside_class_is_true() {
        // /[(){}]*/ — the `(){}` are literal members; the trailing `*` (after the
        // closing `]` has been consumed) is outside.
        // The convention: a content byte is reported "inside" if the scan up to
        // it has seen an unmatched `[`. The closing `]` itself is reported inside
        // (it is consumed only once the scan passes it); the byte after it is out.
        let b = b"[(){}]*";
        // index 0 = `[` itself: scan hasn't opened a class yet.
        assert!(!is_inside_char_class(b, 0));
        // indices 1..=4 = `(`, `)`, `{`, `}` literal members.
        for i in 1..=4 {
            assert!(is_inside_char_class(b, i), "index {i} should be inside the class");
        }
        // index 6 = `*` quantifier, after the closing `]` was consumed — outside.
        assert!(!is_inside_char_class(b, 6), "the quantifier after the class is outside");
    }

    #[test]
    fn first_close_bracket_after_open_is_literal() {
        // /[]$]/ — the first `]` (index 1) is a literal class member, so `$`
        // (index 2) is still inside the class; the byte after the real close
        // (index 3) would be outside.
        let b = b"[]$]X";
        assert!(is_inside_char_class(b, 1), "first ] after [ is a literal member");
        assert!(is_inside_char_class(b, 2), "$ is inside the class");
        assert!(!is_inside_char_class(b, 4), "byte after the second ] is outside");
    }

    #[test]
    fn first_close_bracket_after_negated_open_is_literal() {
        // /[^]$]/ — the first `]` after `[^` is a literal member.
        let b = b"[^]$]X";
        assert!(is_inside_char_class(b, 2), "first ] after [^ is a literal member");
        assert!(is_inside_char_class(b, 3), "$ is inside the class");
        assert!(!is_inside_char_class(b, 5), "byte after the second ] is outside");
    }

    #[test]
    fn escaped_brackets_do_not_open_or_close() {
        // /\[a\]b/ — both brackets are escaped, so nothing is ever inside a class.
        let b = br"\[a\]b";
        for i in 0..b.len() {
            assert!(!is_inside_char_class(b, i), "index {i} should be outside (escaped brackets)");
        }
    }

    #[test]
    fn escaped_close_inside_class_stays_inside() {
        // /[a\]b]X/ — the escaped `]` (indices 2-3) does not close the class,
        // so `b` (index 4) is still inside; the byte after the real close
        // (index 6) is outside.
        let b = br"[a\]b]X";
        assert!(is_inside_char_class(b, 4), "b stays inside after an escaped ]");
        assert!(!is_inside_char_class(b, 6), "byte after the unescaped ] is outside");
    }

    #[test]
    fn does_not_panic_on_trailing_backslash() {
        let b = br"[\";
        // Must not panic with OOB index regardless of target.
        let _ = is_inside_char_class(b, b.len());
    }
}
