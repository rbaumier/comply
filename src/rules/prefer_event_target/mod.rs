//! prefer-event-target

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-event-target",
    description: "Prefer `EventTarget` over `EventEmitter`.",
    remediation: "Use the web-standard `EventTarget` class instead of Node's `EventEmitter` — it works in all runtimes.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
