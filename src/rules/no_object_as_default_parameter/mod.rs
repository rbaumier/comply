//! no-object-as-default-parameter — flag object literals as default parameter values.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-object-as-default-parameter",
    description: "Do not use an object literal as a default parameter value.",
    remediation: "Use destructuring with individual defaults instead of a \
                  default object literal. `function f({ timeout = 1000 } = {})` \
                  is clearer and avoids the all-or-nothing replacement problem \
                  when a caller passes a partial object.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
