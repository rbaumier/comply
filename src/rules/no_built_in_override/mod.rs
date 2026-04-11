//! no-built-in-override

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-built-in-override",
    description: "Overriding built-in globals like `Array`, `Object`, `Promise` shadows critical APIs.",
    remediation: "Rename the variable. Overriding built-in globals breaks standard library behaviour and causes subtle bugs downstream.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
