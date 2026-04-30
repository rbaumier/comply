//! require-post-message-target-origin

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "require-post-message-target-origin",
    description: "`postMessage()` called without the `targetOrigin` argument.",
    remediation: "Always provide a `targetOrigin` argument (e.g. `self.location.origin` or `'*'`) to `postMessage()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
