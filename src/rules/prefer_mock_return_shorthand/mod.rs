//! prefer-mock-return-shorthand — flag `.mockImplementation(() => x)` that
//! simply returns a value and suggest the shorthand `.mockReturnValue(x)`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-mock-return-shorthand",
    description: "Prefer `.mockReturnValue(x)` over `.mockImplementation(() => x)`.",
    remediation: "Use mockReturnValue(x) instead of mockImplementation(() => x)",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
