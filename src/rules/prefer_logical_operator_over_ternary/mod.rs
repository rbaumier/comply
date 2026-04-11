//! prefer-logical-operator-over-ternary — flag `foo ? foo : bar` -> `foo || bar`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-logical-operator-over-ternary",
    description: "Prefer `||`/`??` over a ternary that repeats the test in a branch.",
    remediation: "Replace `foo ? foo : bar` with `foo || bar` (or `foo ?? bar`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
