//! banned-comment-words — flag dismissive filler words in code comments.
//!
//! Words like "obviously", "simply", "just", "basically" are red flags in
//! comments. They paper over complexity without explaining it. The
//! coding-standards skill says: "If it's obvious, no comment is needed; if
//! it needs `simply`, it's not simple." Strip the filler and either delete
//! the comment or rewrite it to explain the actual subtlety.

mod oxc_typescript;
mod rust;
mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "banned-comment-words",
    description: "Dismissive filler words in comments hide complexity instead of explaining it.",
    remediation: "Remove the filler word and rewrite the comment to explain the actual \
                  subtlety. If the line needs no explanation, delete the comment instead. \
                  Banned: obviously, simply, just, basically, clearly, trivially, \
                  reloaded, really, literally, genuinely, honestly, truly, fundamentally, \
                  inevitably, interestingly, importantly, crucially.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["comments"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

const BANNED: &[&str] = &[
    "obviously",
    "simply",
    "just",
    "basically",
    "clearly",
    "trivially",
    "reloaded",
    "really",
    "literally",
    "genuinely",
    "honestly",
    "truly",
    "fundamentally",
    "inevitably",
    "interestingly",
    "importantly",
    "crucially",
];

/// Return the first banned word found in `text` at a word boundary,
/// case-insensitive. Used by both the TS and Rust AST backends; the Vue
/// `text.rs` backend has its own line-scanning logic.
pub(crate) fn find_banned_word(text: &str) -> Option<&'static str> {
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    for &word in BANNED {
        let needle = word.as_bytes();
        if needle.len() > bytes.len() {
            continue;
        }
        let mut i = 0;
        while i + needle.len() <= bytes.len() {
            if &bytes[i..i + needle.len()] == needle {
                let prev_ok = i == 0 || !bytes[i - 1].is_ascii_alphabetic();
                let next_ok = i + needle.len() == bytes.len()
                    || !bytes[i + needle.len()].is_ascii_alphabetic();
                if prev_ok && next_ok {
                    if ends_with_negation(&lower[..i]) {
                        i += 1;
                        continue;
                    }
                    return Some(word);
                }
            }
            i += 1;
        }
    }
    None
}

/// True when `prefix` (the lowercased comment text immediately preceding a
/// banned-word match) ends with an English negation — `not`, `cannot`, or a
/// `…n't` contraction. A negated filler word reverses the dismissive import the
/// rule targets: `not simply` means "does more than merely", which explains
/// complexity rather than papering over it.
pub(crate) fn ends_with_negation(prefix: &str) -> bool {
    let token = prefix
        .trim_end()
        .rsplit(char::is_whitespace)
        .next()
        .unwrap_or("")
        .trim_matches(|c: char| !c.is_ascii_alphabetic() && c != '\'');
    token == "not" || token == "cannot" || token.ends_with("n't")
}

#[cfg(test)]
mod tests {
    use super::find_banned_word;

    #[test]
    fn allows_negated_banned_word_issue_6460() {
        assert_eq!(
            find_banned_word("note that this will not simply filter the entries"),
            None
        );
    }

    #[test]
    fn flags_unnegated_banned_word_still() {
        assert_eq!(find_banned_word("this simply works"), Some("simply"));
        assert_eq!(find_banned_word("just call foo"), Some("just"));
    }

    #[test]
    fn allows_cannot_and_contraction_negation() {
        assert_eq!(find_banned_word("you cannot simply do x"), None);
        assert_eq!(find_banned_word("it doesn't just return"), None);
    }

    #[test]
    fn still_flags_dismissive_word_after_negated_one() {
        // "simply" is negated and skipped, but the un-negated "just" later in
        // the same comment is still caught.
        assert_eq!(find_banned_word("not simply, just do it"), Some("just"));
    }

    #[test]
    fn does_not_exempt_words_ending_in_not() {
        // Token-exact: `knot` must not count as the negation "not".
        assert_eq!(find_banned_word("a knot simply tied"), Some("simply"));
    }
}

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::Tsx,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Vue, Backend::Text(Box::new(text::Check))),
        ],
    }
}
