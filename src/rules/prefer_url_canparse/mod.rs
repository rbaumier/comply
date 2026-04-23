//! Prefer `URL.canParse()` over try-catch `new URL()` (ES2024).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-url-canparse",
    description: "Prefer `URL.canParse(url)` over try-catch with `new URL()`.",
    remediation: "Replace try-catch URL validation with `URL.canParse(url)` (available in modern runtimes).",
    severity: Severity::Warning,
    doc_url: Some("https://developer.mozilla.org/en-US/docs/Web/API/URL/canParse_static"),
    categories: &["e18e", "modernization"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
