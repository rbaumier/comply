//! require-array-join-separator

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "require-array-join-separator",
    description: "Enforce using the separator argument with `Array#join()`.",
    remediation: "Pass an explicit separator: `arr.join(',')`. The default is `','` but relying on it harms readability.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
