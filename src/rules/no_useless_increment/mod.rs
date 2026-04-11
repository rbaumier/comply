//! no-useless-increment

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-useless-increment",
    description: "`return x++` / `return x--` returns the value *before* the increment.",
    remediation: "Increment before the return (`x++; return x;`) or use prefix (`return ++x`).",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
