mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-shallow-passthrough-method",
    description: "Method body only forwards arguments to another method with the same signature.",
    remediation: "Inline the call at each call-site or add real behaviour — a pure pass-through adds a layer with no value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
