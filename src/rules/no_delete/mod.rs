//! no-delete

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-delete",
    description: "Disallow the `delete` operator — it mutates objects in place.",
    remediation: "Build a new object without the property, e.g. `const { [key]: _, ...rest } = obj;` or use `Object.fromEntries(Object.entries(obj).filter(...))`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["functional"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
