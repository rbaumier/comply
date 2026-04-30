mod react;
use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-passive-event-listeners",
    description: "Scroll/touch/wheel listeners should be passive to avoid blocking the main thread.",
    remediation: "Pass `{ passive: true }` as the third argument: `addEventListener('wheel', handler, { passive: true })`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
