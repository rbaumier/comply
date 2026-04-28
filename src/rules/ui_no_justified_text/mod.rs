//! ui-no-justified-text — `textAlign: 'justify'` without `hyphens: 'auto'`
//! produces rivers of whitespace and harms readability.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-justified-text",
    description: "`textAlign: 'justify'` without `hyphens: 'auto'` — produces rivers of whitespace.",
    remediation: "Either drop `textAlign: 'justify'` or pair it with `hyphens: 'auto'` so the \
                  browser can break long words and avoid awkward spacing.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
