//! banned-comment-phrases — flag AI-tell narrator preambles and business
//! jargon in code comments.
//!
//! Phrases like "here's the thing", "let me walk you through" or "deep dive"
//! announce a point instead of stating it. They read as AI-generated prose
//! (see the stop-slop skill) and carry no information a reader of the code
//! needs. Cut the phrase and state the point, or delete the comment.

mod oxc_typescript;
mod rust;
mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "banned-comment-phrases",
    description: "Narrator preambles and business jargon in comments read as AI-generated filler.",
    remediation: "Drop the phrase and state the point directly, or delete the comment. \
                  Banned: here's the thing, here's what, here's why, let me be clear, \
                  let me walk you through, in this section we'll, as we'll see, the \
                  uncomfortable truth, let that sink in, make no mistake, the reality is, \
                  at the end of the day, it's worth noting, think about it, and that's okay, \
                  deep dive, game-changer, circle back, double down, on the same page.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["comments"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

const PHRASES: &[&str] = &[
    "here's the thing",
    "here's what",
    "here's why",
    "let me be clear",
    "let me walk you through",
    "in this section we'll",
    "as we'll see",
    "the uncomfortable truth",
    "let that sink in",
    "make no mistake",
    "the reality is",
    "at the end of the day",
    "it's worth noting",
    "think about it",
    "and that's okay",
    "deep dive",
    "game-changer",
    "circle back",
    "double down",
    "on the same page",
];

/// Return the first banned phrase found in `text`, case-insensitive, matched
/// at outer word boundaries so `deep dive` cannot fire inside a longer word.
/// Phrases carry spaces, apostrophes and hyphens internally; only the leading
/// and trailing characters are boundary-checked. Shared by the TS, Rust and
/// Vue backends.
pub(crate) fn find_banned_phrase(text: &str) -> Option<&'static str> {
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    for &phrase in PHRASES {
        let needle = phrase.as_bytes();
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
                    return Some(phrase);
                }
            }
            i += 1;
        }
    }
    None
}

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Vue, Backend::Text(Box::new(text::Check))),
        ],
    }
}
