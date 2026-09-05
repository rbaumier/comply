mod oxc_typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-documented-with-semantics",
    description: "Complex regex (>20 chars) without a comment explaining its purpose.",
    remediation: "Add a comment above the regex explaining what it matches.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

const MIN_PATTERN_LEN: usize = 20;

/// Characters a regex engine reads as syntax rather than as themselves.
const METACHARACTERS: &[char] =
    &['.', '*', '+', '?', '(', ')', '[', ']', '|', '{', '}', '^', '$'];

/// True if `pattern` matches literal text only — one fixed string, or a
/// `|`-separated list of fixed strings, optionally bracketed by `^` and `$`
/// anchors. Such a regex IS its own documentation: a comment can only restate
/// the strings it spells out.
///
/// Escaping is spelling, not meaning: `/Error: sendMessage\(\) failed/` matches
/// the plain sentence `Error: sendMessage() failed`, so the backslashes make it
/// no more complex than the sentence itself.
pub(crate) fn is_literal_pattern(pattern: &str) -> bool {
    let inner = strip_anchors(pattern);
    !inner.is_empty()
        && split_alternatives(inner)
            .into_iter()
            .all(|alternative| literal_text(alternative).is_some_and(|text| !text.is_empty()))
}

/// The exact text `pattern` matches, when it matches one fixed string.
///
/// A backslash before an ASCII punctuation character strips that character's
/// syntactic role and leaves the character itself, so `\(` contributes a literal
/// `(`. A backslash before anything else keeps its meaning — `\d` and `\w` are
/// classes, `\b` an anchor, `\1` a back-reference, a `u` prefix a code point —
/// and yields `None`, as do a bare metacharacter and a trailing lone backslash.
fn literal_text(pattern: &str) -> Option<String> {
    let mut text = String::with_capacity(pattern.len());
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let escaped = chars.next()?;
                if !escaped.is_ascii_punctuation() {
                    return None;
                }
                text.push(escaped);
            }
            _ if METACHARACTERS.contains(&c) => return None,
            _ => text.push(c),
        }
    }
    Some(text)
}

/// `pattern` without the `^` and `$` that bracket it. Nothing can precede the
/// opening `^`, so it always anchors; a closing `$` that an odd run of
/// backslashes escapes is a literal dollar sign and stays in the pattern.
fn strip_anchors(pattern: &str) -> &str {
    let inner = pattern.strip_prefix('^').unwrap_or(pattern);
    match inner.strip_suffix('$') {
        Some(without_anchor) if !ends_with_escape(without_anchor) => without_anchor,
        _ => inner,
    }
}

/// True when `pattern` ends on a backslash that escapes what follows it — an
/// odd run of trailing backslashes, each pair of which is itself an escaped
/// backslash.
fn ends_with_escape(pattern: &str) -> bool {
    pattern.chars().rev().take_while(|c| *c == '\\').count() % 2 == 1
}

/// The alternatives `pattern` offers, split on its top-level `|`.
/// A `\|` spells a literal pipe and opens no alternative.
fn split_alternatives(pattern: &str) -> Vec<&str> {
    let mut alternatives = Vec::new();
    let mut start = 0;
    let mut after_backslash = false;
    for (index, c) in pattern.char_indices() {
        match c {
            _ if after_backslash => after_backslash = false,
            '\\' => after_backslash = true,
            '|' => {
                alternatives.push(&pattern[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    alternatives.push(&pattern[start..]);
    alternatives
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn accepts_plain_anchored_sentence() {
        assert!(is_literal_pattern(
            "^Type invalide : chaîne attendu, nombre reçu$"
        ));
    }

    #[test]
    fn accepts_partial_anchor() {
        assert!(is_literal_pattern("^starts here"));
        assert!(is_literal_pattern("ends here$"));
    }

    #[test]
    fn rejects_pattern_with_metacharacters() {
        assert!(!is_literal_pattern("^\\d+ users$"));
        assert!(!is_literal_pattern("^(foo|bar)$"));
        assert!(!is_literal_pattern("^abc.*xyz$"));
    }

    #[test]
    fn rejects_empty_pattern() {
        assert!(!is_literal_pattern(""));
        assert!(!is_literal_pattern("^$"));
    }

    #[test]
    fn accepts_pure_literal_alternation() {
        assert!(is_literal_pattern(
            "jiti|node:internal|citty|listhen|listenAndWatch"
        ));
        assert!(is_literal_pattern("^foo|bar|baz$"));
    }

    #[test]
    fn rejects_alternation_with_metacharacter_alternative() {
        assert!(!is_literal_pattern("foo|ba.r"));
        assert!(!is_literal_pattern("a|b+"));
        assert!(!is_literal_pattern("(a|b)+"));
        assert!(!is_literal_pattern("[a-z]+"));
    }

    #[test]
    fn rejects_alternation_with_empty_alternative() {
        assert!(!is_literal_pattern("foo||bar"));
        assert!(!is_literal_pattern(""));
    }

    #[test]
    fn accepts_escaped_punctuation_as_literal() {
        // Regression for rbaumier/comply#8243 — `\(` matches a parenthesis, so
        // the pattern spells a plain sentence and documents itself.
        assert!(is_literal_pattern("Error: sendMessage\\(\\) cannot be used"));
        assert!(is_literal_pattern("^a\\+b\\*c\\?d\\|e\\/f is long enough$"));
        assert!(is_literal_pattern("`stdio\\[3\\]\\.input` option must use a boolean"));
    }

    #[test]
    fn accepts_alternation_of_escaped_literals() {
        assert!(is_literal_pattern("^a|b\\.c|d is long enough$"));
        assert!(!is_literal_pattern("^a|b.c|d is long enough$"));
    }

    #[test]
    fn rejects_escapes_that_keep_their_meaning() {
        assert!(!is_literal_pattern("\\d{2}:\\d{2} something long enough"));
        assert!(!is_literal_pattern("^fd(?<fdNumber>\\d+)$"));
        assert!(!is_literal_pattern("\\bword\\b boundaries here"));
        assert!(!is_literal_pattern("ends on a lone backslash \\"));
    }

    #[test]
    fn keeps_an_escaped_dollar_out_of_the_anchor() {
        assert!(is_literal_pattern("costs 100\\$"));
        assert!(is_literal_pattern("^costs 100\\$$"));
    }
}

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
