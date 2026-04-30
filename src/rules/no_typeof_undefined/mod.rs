//! no-typeof-undefined — flag `typeof x === 'undefined'`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-typeof-undefined",
    description: "Prefer direct `=== undefined` comparison when the operand is \
                  guaranteed to be a declared binding (e.g. a property access).",
    remediation: "When the operand is a member expression like `obj.foo`, \
                  replace `typeof obj.foo === 'undefined'` with \
                  `obj.foo === undefined`. Keep `typeof` when the operand is a \
                  bare identifier that may not be declared — \
                  `x === undefined` throws ReferenceError in that case.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
