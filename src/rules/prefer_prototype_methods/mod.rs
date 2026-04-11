//! prefer-prototype-methods — borrow methods from prototypes, not literal instances.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-prototype-methods",
    description: "Prefer borrowing methods from the prototype instead of a literal instance.",
    remediation:
        "Replace `{}.hasOwnProperty.call(…)` with `Object.prototype.hasOwnProperty.call(…)`, \
                  `[].slice.call(…)` with `Array.prototype.slice.call(…)`, etc.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
