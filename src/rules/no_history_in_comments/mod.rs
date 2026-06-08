mod oxc_typescript;
mod rust;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-history-in-comments",
    description: "Comment narrates history rather than describing current behaviour.",
    remediation: "Keep comments about what the code does now. Put history in git log or commit messages.",
    severity: Severity::Warning,
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

const HISTORY_PHRASES: &[&str] = &[
    "was changed",
    "was modified",
    "was removed",
    "was deleted",
    "was replaced",
    "was refactored",
    "was rewritten",
    "was moved",
    "was renamed",
    "was updated",
    "was converted",
    "was migrated",
    "previously used",
    "previously called",
    "previously stored",
    "previously named",
    "previously returned",
    "previously implemented",
];

const HISTORY_WORDS_ALWAYS: &[&str] = &["refactored", "rewritten"];

pub(crate) fn mentions_history(raw: &str) -> bool {
    if raw.starts_with("///") || raw.starts_with("//!") || raw.starts_with("/**") {
        return false;
    }
    let lower = raw.to_lowercase();
    if HISTORY_PHRASES.iter().any(|p| lower.contains(p)) {
        return true;
    }
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    for (i, word) in words.iter().enumerate() {
        if HISTORY_WORDS_ALWAYS.contains(word) {
            // "be rewritten" / "be refactored" is a verb describing expected
            // (often negated or hypothetical) behaviour, not a past code change.
            if i.checked_sub(1).map(|j| words[j]) == Some("be") {
                continue;
            }
            return true;
        }
    }
    false
}
