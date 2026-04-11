//! no-this-assignment — flag `const self = this` patterns.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-this-assignment",
    description: "Disallow assigning `this` to a variable.",
    remediation: "Use an arrow function instead of capturing `this` in a \
                  variable. Arrow functions lexically bind `this`, making \
                  the alias unnecessary and removing a common source of bugs.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
