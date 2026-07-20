//! Shared ReDoS shape discriminator for the regex lint rules.
//!
//! `regex-no-slow-pattern` and `security-detect-unsafe-regex` both flag a group
//! quantified by `*`/`+` that also contains an inner `*`/`+`. That shape is only
//! catastrophic when the inner and outer repetitions can match the *same* input
//! at the *same* position (`(a+)+`, `(.*)*`, `(aa+)+`). This module spares the
//! one provably linear case: the "unrolling the loop" idiom, whose inner
//! quantifier is anchored behind a mandatory literal *disjoint* from the
//! repeated class (`<[^<]*`), so every anchor is a hard partition boundary.

use crate::rules::regex_helpers::{
    atom_body_len, char_class_len, group_len, scan_atom, strip_group_prefix,
};

/// Returns true when the outer-quantified group `body` (bytes between its `(`
/// and matching `)`) is an *ambiguous* quantified atom — the evil ReDoS shape
/// where the outer repetition overlaps an inner one. It is ambiguous when the
/// first mandatory-position consuming atom is directly repeated by `*`/`+`
/// (`(a+)+`, `(.*)*`, `([a-z]+)*`) or is itself such a group (`((a+))+`). The
/// non-consuming group prefix, leading zero-width lookarounds, and nullable
/// (optional) leading atoms are skipped first — none can anchor the repetition.
///
/// It returns false only for the linear "unrolling the loop" idiom
/// `(?:(?!</tag>)<[^<]*)*`: a mandatory literal excluded by the following
/// repeated negated class. That literal is a hard partition boundary, so no
/// exponential re-partitioning exists. A mandatory atom that overlaps the
/// repeated one (`(aa+)+`, `([a-z][a-z]+)+`) is still catastrophic → true.
#[must_use]
pub(crate) fn inner_quantifier_on_leading_atom(body: &[u8]) -> bool {
    let mut body = strip_group_prefix(body);
    while starts_with_lookaround(body) {
        body = &body[group_len(body, 0)..];
    }
    while let Some(&first) = body.first() {
        let len = atom_body_len(body, 0);
        // A leading atom repeated directly by `*`/`+`, or a leading group that
        // is itself an ambiguous quantified atom, keeps the ReDoS flag.
        let directly_repeated = matches!(body.get(len), Some(b'*' | b'+'));
        let closed_group = first == b'(' && body.get(len - 1) == Some(&b')');
        let ambiguous_group = closed_group && inner_quantifier_on_leading_atom(&body[1..len - 1]);
        if directly_repeated || ambiguous_group {
            return true;
        }
        // The linear "unrolling the loop" idiom: a literal excluded by a
        // following repeated negated class anchors every iteration → not evil.
        if is_unrolled_loop_anchor(body, len) {
            return false;
        }
        // Skip a nullable (optional) leading atom; it can't anchor. A mandatory
        // atom that is not a proven-disjoint anchor stays conservatively flagged.
        let (atom_len, nullable) = scan_atom(body, 0);
        if !nullable {
            return true;
        }
        body = &body[atom_len..];
    }
    false
}

/// True if `body` starts with a zero-width lookaround (`(?=`/`(?!`/`(?<=`/`(?<!`).
fn starts_with_lookaround(body: &[u8]) -> bool {
    body.first() == Some(&b'(')
        && body.get(1) == Some(&b'?')
        && match body.get(2) {
            Some(b'=' | b'!') => true,
            Some(b'<') => matches!(body.get(3), Some(b'=' | b'!')),
            _ => false,
        }
}

/// True if `body` begins with the linear "unrolling the loop" tag-stripper
/// shape: a single mandatory literal `L` (`l_len == 1`) immediately followed by
/// a repeated negated class `[^…L…]*`/`+` that excludes `L`. `L` disjoint from
/// the repeated class makes every `L` a hard partition boundary — no
/// catastrophic backtracking.
fn is_unrolled_loop_anchor(body: &[u8], l_len: usize) -> bool {
    if l_len != 1 || matches!(body[0], b'.' | b'^' | b'$') {
        return false;
    }
    if body.get(1) != Some(&b'[') || body.get(2) != Some(&b'^') {
        return false;
    }
    let class_len = char_class_len(body, 1);
    if !matches!(body.get(1 + class_len), Some(b'*' | b'+')) {
        return false;
    }
    let Some(excluded) = body.get(3..class_len) else {
        return false;
    };
    // Trust byte-membership only for a plain-literal class: a `-` range or a `\`
    // escape (`[^a-z]`, `[^\d]`) can match `L` even though `L`'s byte appears in
    // the class source, so a byte match there does not prove `L` is excluded.
    let is_plain_literal_class = !excluded.contains(&b'\\') && !excluded.contains(&b'-');
    is_plain_literal_class && excluded.contains(&body[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discriminates_ambiguous_from_anchored_quantifier() {
        // Ambiguous (keep the ReDoS flag): the first mandatory-position atom is
        // directly repeated, or is a group that is itself such a shape.
        assert!(inner_quantifier_on_leading_atom(b"a+"), "(a+)+");
        assert!(inner_quantifier_on_leading_atom(br"\d+"), "(\\d+)+");
        assert!(inner_quantifier_on_leading_atom(b"[a-z]+"), "([a-z]+)*");
        assert!(inner_quantifier_on_leading_atom(b".*"), "(.*)*");
        assert!(inner_quantifier_on_leading_atom(b"(a|b)+"), "((a|b)+)+");
        assert!(inner_quantifier_on_leading_atom(b"(a+)"), "((a+))+ recurses into leading group");
        assert!(inner_quantifier_on_leading_atom(br"a?b+"), "(a?b+)+ optional atom can't anchor");
        assert!(inner_quantifier_on_leading_atom(br"\s?\w+"), "(\\s?\\w+)+ nullable separator");
        // Non-disjoint anchor: the mandatory atom overlaps the repeated one, so
        // it does not partition the input → still catastrophic, keep the flag.
        assert!(inner_quantifier_on_leading_atom(b"aa+"), "(aa+)+");
        assert!(inner_quantifier_on_leading_atom(b"[a-z][a-z]+"), "([a-z][a-z]+)+");
        assert!(inner_quantifier_on_leading_atom(b"ab+"), "distinct literal not proven disjoint");
        // Byte appears in the class source but a `-` range / `\` escape still
        // matches `L`, so `L` is NOT excluded → overlapping → keep the flag.
        assert!(inner_quantifier_on_leading_atom(b"-[^a-z]*"), "(-[^a-z]*)+ range hyphen");
        assert!(inner_quantifier_on_leading_atom(br"d[^\d]*"), "(d[^\\d]*)+ escape class");
        // Malformed unterminated leading `(` must not panic.
        assert!(inner_quantifier_on_leading_atom(b"("), "unterminated group is conservative");
        // Anchored (suppress): a literal excluded by the following repeated
        // negated class — the linear unrolled-loop tag stripper.
        assert!(!inner_quantifier_on_leading_atom(br"?:(?!<\/script>)<[^<]*"));
        assert!(!inner_quantifier_on_leading_atom(b"<[^<]*"), "`<` excluded by `[^<]*`");
        assert!(!inner_quantifier_on_leading_atom(br#""[^"]*"#), "quoted-string stripper");
    }
}
