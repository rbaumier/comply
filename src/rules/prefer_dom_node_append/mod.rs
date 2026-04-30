//! prefer-dom-node-append

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-dom-node-append",
    description: "Prefer `Node#append()` over `Node#appendChild()`.",
    remediation: "Replace `.appendChild(x)` with `.append(x)`. \
                  `.append()` accepts multiple arguments, strings, and \
                  never returns the appended node (avoiding subtle misuse).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
