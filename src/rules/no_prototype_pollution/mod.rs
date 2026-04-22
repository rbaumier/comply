mod typescript;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-prototype-pollution",
    description: "Deep-merging user-supplied objects can pollute `Object.prototype`.",
    remediation: "Validate/sanitize input before merging, or use a safe merge that rejects `__proto__` keys.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
