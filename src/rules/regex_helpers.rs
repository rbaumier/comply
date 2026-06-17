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

/// Returns true if a group body can match the empty string (is *nullable*).
///
/// `body` is the bytes between a group's `(` and its matching `)`, including any
/// leading group prefix (`?:`, `?<name>`, `?=`, `?!`, `?<=`, `?<!`), which is
/// stripped here. Nullability follows regex syntax:
/// - the body is an ALTERNATION (top-level `|`), nullable iff ANY branch is;
/// - each branch is a CONCATENATION, nullable iff EVERY atom is;
/// - an atom is nullable iff suffixed by `?`, `*`, or `{0,…}` (min 0), or it is
///   itself a nullable group; an unsuffixed literal / `\escape` / `[char-class]`
///   / `.` / `+`-or-`{≥1}`-quantified group is MANDATORY and makes its branch
///   non-nullable.
///
/// `|` and quantifier characters inside a `[...]` character class are literal
/// members, not syntax, so they neither split alternations nor mark atoms.
#[must_use]
pub fn group_is_nullable(body: &[u8]) -> bool {
    let body = strip_group_prefix(body);
    // Alternation: nullable iff any top-level branch is nullable.
    split_top_level_alternation(body).into_iter().any(branch_is_nullable)
}

/// Strips a leading group prefix so only the matchable body remains:
/// `?:`, `?=`, `?!`, `?<=`, `?<!`, `?<name>`. A bare `(...)` capture group has
/// no prefix and is returned unchanged.
fn strip_group_prefix(body: &[u8]) -> &[u8] {
    if body.first() != Some(&b'?') {
        return body;
    }
    match body.get(1) {
        Some(b':' | b'=' | b'!') => &body[2..],
        Some(b'<') => match body.get(2) {
            // Lookbehind `?<=` / `?<!`.
            Some(b'=' | b'!') => &body[3..],
            // Named group `?<name>` — strip through the closing `>`.
            _ => match body[2..].iter().position(|&c| c == b'>') {
                Some(rel) => &body[2 + rel + 1..],
                None => body,
            },
        },
        _ => body,
    }
}

/// Splits a group body into top-level alternation branches on unescaped `|`
/// that are not inside a nested group or a `[...]` character class.
fn split_top_level_alternation(body: &[u8]) -> Vec<&[u8]> {
    let mut branches = Vec::new();
    let mut start = 0;
    let mut depth = 0u32;
    let mut i = 0;
    while i < body.len() {
        match body[i] {
            b'\\' => {
                i += 2;
                continue;
            }
            b'(' if !is_inside_char_class(body, i) => depth += 1,
            b')' if depth > 0 && !is_inside_char_class(body, i) => depth -= 1,
            b'|' if depth == 0 && !is_inside_char_class(body, i) => {
                branches.push(&body[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    branches.push(&body[start..]);
    branches
}

/// A concatenation branch is nullable iff every atom in it is nullable.
fn branch_is_nullable(branch: &[u8]) -> bool {
    let mut i = 0;
    while i < branch.len() {
        // Length of the atom starting at `i` and whether it is nullable.
        let (atom_len, nullable) = scan_atom(branch, i);
        if !nullable {
            return false;
        }
        i += atom_len;
    }
    true
}

/// Scans the atom starting at `branch[i]` and returns `(byte length including
/// any trailing quantifier, is the atom nullable)`. Caller guarantees
/// `i < branch.len()`.
fn scan_atom(branch: &[u8], i: usize) -> (usize, bool) {
    let body_len = match branch[i] {
        // Escaped single char: `\d`, `\n`, `\r`, … (always consumes ≥1 char).
        b'\\' => 2.min(branch.len() - i),
        // Char class `[...]` — find its unescaped close; always consumes ≥1.
        b'[' => char_class_len(branch, i),
        // Nested group — find its matching `)`; nullability is recursive.
        b'(' => group_len(branch, i),
        // Any other single byte (literal, `.`, `^`, `$`, …).
        _ => 1,
    };
    let (quant_len, min_zero) = scan_quantifier(branch, i + body_len);
    let total = body_len + quant_len;
    if min_zero {
        // `?`, `*`, `{0,…}` → atom is nullable regardless of its body.
        return (total, true);
    }
    // No min-0 quantifier: nullable only if the atom body itself is nullable,
    // which is only possible for a nested group.
    let nullable = branch[i] == b'(' && group_is_nullable(&branch[i + 1..i + body_len - 1]);
    (total, nullable)
}

/// Returns the byte length of the `[...]` char class starting at `start`,
/// including the closing `]`. If unterminated, spans to the end.
fn char_class_len(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    // The first `]` immediately after `[` or `[^` is a literal member.
    if bytes.get(i) == Some(&b'^') {
        i += 1;
    }
    if bytes.get(i) == Some(&b']') {
        i += 1;
    }
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b']' => return i + 1 - start,
            _ => i += 1,
        }
    }
    bytes.len() - start
}

/// Returns the byte length of the group starting at `start` (`(`), including
/// the matching `)`. If unterminated, spans to the end.
fn group_len(bytes: &[u8], start: usize) -> usize {
    let mut depth = 0u32;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'(' if !is_inside_char_class(bytes, i) => {
                depth += 1;
                i += 1;
            }
            b')' if !is_inside_char_class(bytes, i) => {
                depth -= 1;
                if depth == 0 {
                    return i + 1 - start;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    bytes.len() - start
}

/// Scans a quantifier at `pos` and returns `(byte length, has min 0)`.
/// Recognises `?`, `*`, `+`, and `{m}` / `{m,}` / `{m,n}`. A trailing lazy `?`
/// (`*?`, `+?`, `{m,n}?`) is folded into the length. `?` and `*` have min 0;
/// `{0,…}` has min 0; `+` and `{≥1,…}` do not.
fn scan_quantifier(bytes: &[u8], pos: usize) -> (usize, bool) {
    match bytes.get(pos) {
        Some(b'?' | b'*') => (1 + lazy_suffix(bytes, pos + 1), true),
        Some(b'+') => (1 + lazy_suffix(bytes, pos + 1), false),
        Some(b'{') => {
            let Some(rel) = bytes[pos..].iter().position(|&c| c == b'}') else {
                return (0, false);
            };
            let inner = &bytes[pos + 1..pos + rel];
            let min_str: &[u8] = match inner.iter().position(|&c| c == b',') {
                Some(comma) => &inner[..comma],
                None => inner,
            };
            // Reject non-numeric `{...}` (literal braces) — not a quantifier.
            if min_str.is_empty() || !min_str.iter().all(u8::is_ascii_digit) {
                return (0, false);
            }
            let min_zero = min_str.iter().all(|&c| c == b'0');
            let len = rel + 1 + lazy_suffix(bytes, pos + rel + 1);
            (len, min_zero)
        }
        _ => (0, false),
    }
}

/// Returns 1 if a lazy `?` immediately follows a quantifier at `pos`, else 0.
fn lazy_suffix(bytes: &[u8], pos: usize) -> usize {
    usize::from(bytes.get(pos) == Some(&b'?'))
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

    // --- group_is_nullable ---

    #[test]
    fn nullable_bodies() {
        assert!(group_is_nullable(b"a?"), "single optional atom");
        assert!(group_is_nullable(b"a*"), "single star atom");
        assert!(group_is_nullable(b"a?b?"), "every atom optional");
        assert!(group_is_nullable(b"a?|b"), "one alternation branch nullable");
        assert!(group_is_nullable(b""), "empty body matches empty");
        assert!(group_is_nullable(b"?:a?"), "non-capturing prefix stripped");
        assert!(group_is_nullable(b"a{0,3}"), "min-0 brace quantifier");
        assert!(group_is_nullable(b"(?:a?)"), "nested all-optional group");
        assert!(group_is_nullable(b"\\r?"), "escaped char made optional");
    }

    #[test]
    fn non_nullable_bodies() {
        assert!(!group_is_nullable(b"a"), "single mandatory literal");
        assert!(!group_is_nullable(b"\\n"), "mandatory escape");
        assert!(!group_is_nullable(b"\\r?\\n"), "optional CR then mandatory LF");
        assert!(!group_is_nullable(b"a?b"), "trailing mandatory atom");
        assert!(!group_is_nullable(b"ab?"), "leading mandatory atom");
        assert!(!group_is_nullable(b"a|b"), "no nullable alternation branch");
        assert!(!group_is_nullable(b"[abc]"), "char class consumes one char");
        assert!(!group_is_nullable(b"a{1,3}"), "min-1 brace quantifier");
        assert!(!group_is_nullable(b"(?:a)+"), "min-1 quantified nested group");
    }

    #[test]
    fn char_class_chars_are_literals() {
        // `|` and quantifiers inside `[...]` are members, not syntax: this body
        // is a single mandatory char class, so it is not nullable.
        assert!(!group_is_nullable(b"[a|b]"), "pipe inside class is a literal");
        assert!(!group_is_nullable(b"[a?]"), "question mark inside class is a literal");
    }
}
