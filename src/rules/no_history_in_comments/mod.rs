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
    lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|word| HISTORY_WORDS_ALWAYS.contains(&word))
}
