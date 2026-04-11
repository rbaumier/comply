//! prefer-string-replace-all

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-string-replace-all",
    description: "Prefer `String#replaceAll()` over `String#replace()` with a global regex.",
    remediation: "Replace `.replace(/pattern/g, replacement)` with `.replaceAll('pattern', replacement)`. \
                  `replaceAll()` is clearer in intent and avoids regex escaping pitfalls.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
