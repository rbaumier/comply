//! ts-prefer-literal-enum-member — require all enum members to be literal values.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-prefer-literal-enum-member",
    description: "Enum members should be initialized with literal values, not computed expressions.",
    remediation: "Replace the computed expression with a literal string or number value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
