//! prefer-add-event-listener

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-add-event-listener",
    description: "Prefer `.addEventListener()` over `on`-event property assignment.",
    remediation: "Replace `element.onclick = handler` with `element.addEventListener('click', handler)`. `addEventListener` supports multiple listeners and provides better control via options (capture, passive, once).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
