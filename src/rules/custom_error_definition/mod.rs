//! custom-error-definition — enforce correct Error subclassing.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "custom-error-definition",
    description: "Enforce correct Error subclassing.",
    remediation: "Use a class field `name = 'MyError';` instead of setting \
                  `this.name` in the constructor. Pass the error message to \
                  `super()` instead of setting `this.message`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
