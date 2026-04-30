//! prefer-dom-node-text-content

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-dom-node-text-content",
    description: "Prefer `.textContent` over `.innerText`.",
    remediation: "Replace `.innerText` with `.textContent`. \
                  `.textContent` is faster (no layout reflow), works on all \
                  node types, and returns text from hidden elements too.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
