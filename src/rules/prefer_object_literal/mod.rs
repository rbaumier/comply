//! prefer-object-literal

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-object-literal",
    description: "Use `{}` instead of `new Object()`.",
    remediation: "Replace `new Object()` with `{}` — object literals are cleaner and more idiomatic.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
