//! prefer-dom-node-remove

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-dom-node-remove",
    description: "Prefer `childNode.remove()` over `parentNode.removeChild(childNode)`.",
    remediation: "Replace `parent.removeChild(child)` with `child.remove()`. \
                  The modern `.remove()` API is simpler and doesn't require \
                  a reference to the parent node.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
