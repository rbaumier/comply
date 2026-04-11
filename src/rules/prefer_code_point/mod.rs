//! prefer-code-point

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-code-point",
    description: "Prefer `String#codePointAt()` over `String#charCodeAt()` and `String.fromCodePoint()` over `String.fromCharCode()`.",
    remediation: "Use `codePointAt()` instead of `charCodeAt()` and `String.fromCodePoint()` instead of `String.fromCharCode()`. The code-point variants handle full Unicode (including astral symbols) correctly.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
