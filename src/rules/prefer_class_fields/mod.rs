//! prefer-class-fields — flag `this.x = <literal>` in constructors.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-class-fields",
    description: "Prefer class field declarations over `this` assignments in constructors for static values.",
    remediation: "Move the literal assignment from the constructor to a class \
                  field declaration. Class fields are more declarative and \
                  make the default value visible at a glance.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
