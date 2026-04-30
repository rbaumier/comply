//! regex-no-single-char-class

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-single-char-class",
    description: "Character class with a single character is unnecessary.",
    remediation: "Replace `[x]` with `x` (or `\\.` for `[.]`). Single-character classes add visual noise without changing semantics.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
