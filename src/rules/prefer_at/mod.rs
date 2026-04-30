//! prefer-at

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-at",
    description: "Prefer `.at()` method for index access and `String#charAt()`.",
    remediation: "Use `.at(-1)` instead of `[arr.length - 1]` for last-element access, and `str.at(0)` instead of `str.charAt(0)`. The `.at()` method handles negative indices natively.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
