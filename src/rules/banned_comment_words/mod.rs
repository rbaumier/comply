//! banned-comment-words — flag dismissive filler words in code comments.
//!
//! Words like "obviously", "simply", "just", "basically" are red flags in
//! comments. They paper over complexity without explaining it. The
//! coding-standards skill says: "If it's obvious, no comment is needed; if
//! it needs `simply`, it's not simple." Strip the filler and either delete
//! the comment or rewrite it to explain the actual subtlety.
//!
//! A banned spelling is filler only in the sense its neighbours give it, and
//! only where no explanation stands beside it: a match inside a negated verb
//! group, or inside a comment block that already spends more words than
//! `comment-max-block-words` allows, is left alone.

mod oxc_typescript;
mod rust;
mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::{Backend, CheckCtx};
use crate::rules::comment_blocks::{self, RawComment};
use crate::rules::meta::RuleMeta;
use rustc_hash::FxHashSet;

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

/// Return the earliest banned word in list order that `text` holds at a word
/// boundary in its dismissive sense, case-insensitive, with the byte offset it
/// starts at. Every backend scans through this function; the AST backends
/// anchor their diagnostic on the offset.
///
/// The offset indexes `text` directly: ASCII lowercasing rewrites ASCII bytes
/// in place and leaves every other byte alone, so positions in the lowercased
/// copy are positions in `text`. It also lands on a `char` boundary, since a
/// match starts on an ASCII letter and no UTF-8 continuation byte is ASCII.
pub(crate) fn find_banned_word(text: &str) -> Option<(&'static str, usize)> {
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    for &word in BANNED {
        let needle = word.as_bytes();
        if needle.len() > bytes.len() {
            continue;
        }
        let mut i = 0;
        while i + needle.len() <= bytes.len() {
            let end = i + needle.len();
            if &bytes[i..end] == needle
                && (i == 0 || !bytes[i - 1].is_ascii_alphabetic())
                && (end == bytes.len() || !bytes[end].is_ascii_alphabetic())
                && !is_negated(&lower[..i], &lower[end..])
            {
                return Some((word, i));
            }
            i += 1;
        }
    }
    None
}

/// The source rows of every comment block in `comments` that spends more than
/// `max` words.
///
/// The rule fires where a filler word stands in place of an explanation. A
/// block past the `comment-max-block-words` budget has spent what an
/// explanation costs, and that rule already reports it for the length — so the
/// same block cannot also be under-explanatory, and a match inside it is left
/// to the rule that measured it.
pub(crate) fn explained_rows(
    comments: Vec<RawComment>,
    source: &str,
    max: usize,
) -> FxHashSet<usize> {
    comment_blocks::merge(comments, source)
        .iter()
        .filter(|block| block.exceeds_budget(max))
        .flat_map(|block| block.lines.iter().map(|line| line.line))
        .collect()
}

/// The word budget past which a comment block counts as an explanation, read
/// off `comment-max-block-words` so the two rules cannot disagree about which
/// blocks are long enough to explain themselves.
pub(crate) fn explanation_budget(ctx: &CheckCtx) -> usize {
    ctx.config.threshold(
        crate::rules::comment_max_block_words::META.id,
        "max",
        ctx.lang,
    )
}

/// True when the match sits inside a negated verb group.
///
/// A negated filler word reverses the dismissive import the rule targets:
/// `not simply` means "does more than merely", which explains complexity
/// rather than papering over it. English puts the adverb on either side of the
/// negation with no change of meaning, and the negator's position is fixed by
/// the grammar: it either sits next to the adverb (`can't really`, `does not
/// simply`, `not simply`) or is carried by the auxiliary the adverb precedes
/// (`simply does not`). Reading further than that verb group would exempt any
/// adverb that merely shares a sentence with a negation.
fn is_negated(before: &str, after: &str) -> bool {
    let next = following_word(after, 0);
    is_negator(preceding_word(before))
        || is_negator(next)
        || (is_auxiliary(next) && is_negator(following_word(after, 1)))
}

/// True for the English negators, `…n't` contractions included.
fn is_negator(token: &str) -> bool {
    matches!(token, "not" | "cannot" | "no") || token.ends_with("n't")
}

/// True for the auxiliary verbs that carry the `not` of a negated verb group.
fn is_auxiliary(token: &str) -> bool {
    matches!(
        token,
        "do" | "does"
            | "did"
            | "is"
            | "are"
            | "was"
            | "were"
            | "am"
            | "be"
            | "been"
            | "being"
            | "have"
            | "has"
            | "had"
            | "can"
            | "could"
            | "will"
            | "would"
            | "shall"
            | "should"
            | "may"
            | "might"
            | "must"
    )
}

/// The word `prefix` ends on, empty when it holds none.
fn preceding_word(prefix: &str) -> &str {
    prefix.split_whitespace().next_back().map_or("", bare_word)
}

/// The `n`th word of `suffix`, counting from zero, empty when it holds fewer.
fn following_word(suffix: &str, n: usize) -> &str {
    suffix.split_whitespace().nth(n).map_or("", bare_word)
}

/// `token` without the punctuation around it. An apostrophe is kept: it is what
/// tells `doesn't` and `we've` apart from `does` and `we`.
fn bare_word(token: &str) -> &str {
    token.trim_matches(|c: char| !c.is_ascii_alphabetic() && c != '\'')
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
        assert_eq!(find_banned_word("this simply works"), Some(("simply", 5)));
        assert_eq!(find_banned_word("just call foo"), Some(("just", 0)));
    }

    #[test]
    fn reports_the_offset_of_a_match_on_a_later_line() {
        // The offset locates the word inside the comment, which is what the AST
        // backends anchor on. A word on the third line of a block comment sits
        // past both newlines.
        assert_eq!(
            find_banned_word("/* first line\nsecond line\nthird is just wrong */"),
            Some(("just", 35))
        );
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
        assert_eq!(find_banned_word("not simply, just do it"), Some(("just", 12)));
    }

    #[test]
    fn does_not_exempt_words_ending_in_not() {
        // Token-exact: `knot` must not count as the negation "not".
        assert_eq!(find_banned_word("a knot simply tied"), Some(("simply", 7)));
    }

    #[test]
    fn allows_negation_following_the_filler_word_issue_8184() {
        // English puts the adverb on either side of the negation with no change
        // of meaning, so both orders have to agree.
        assert_eq!(find_banned_word("i really can't see a better way"), None);
        assert_eq!(find_banned_word("i can't really see a better way"), None);
        assert_eq!(
            find_banned_word("this simply does not filter the entries"),
            None
        );
        assert_eq!(
            find_banned_word("this does not simply filter the entries"),
            None
        );
    }

    #[test]
    fn flags_a_filler_word_whose_verb_group_holds_no_negation() {
        // The window is the verb group. A negation further along the sentence
        // governs its own verb, not this adverb.
        assert_eq!(
            find_banned_word("just call foo and it will not fail"),
            Some(("just", 0))
        );
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
