//! no-constructor-side-effects

//! no-constructor-side-effects

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-constructor-side-effects",
    description: "`new X()` without assignment is a side-effect anti-pattern.",
    remediation: "Assign the result of `new X()` to a variable, or refactor side effects out of the constructor into a static method.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
