//! number-literal-case

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "number-literal-case",
    description: "Enforce proper case for numeric literals.",
    remediation: "Use lowercase for prefix/exponent (`0x`, `0b`, `0o`, `1e3`) and uppercase for hex digits (`0xFF`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
