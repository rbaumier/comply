//! no-pseudo-random

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-pseudo-random",
    description: "`Math.random()` is not cryptographically secure.",
    remediation: "Use `crypto.randomUUID()` or `crypto.getRandomValues()` instead of `Math.random()` for security-sensitive contexts.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
