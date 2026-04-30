mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-history-in-comments",
    description: "Comment narrates history rather than describing current behaviour.",
    remediation: "Keep comments about what the code does now. Put history in git log or commit messages.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}

const HISTORY_WORDS: &[&str] = &["was", "previously", "refactored", "rewritten"];

/// True if a comment's lowercased text contains a history-narrating word as a
/// standalone token. Matching at word boundaries avoids false positives on
/// `waste`, `iteratively`, and similar.
pub(crate) fn mentions_history(raw: &str) -> bool {
    let lower = raw.to_lowercase();
    lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|word| HISTORY_WORDS.contains(&word))
}
