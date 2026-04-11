//! prefer-bigint-literals

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-bigint-literals",
    description: "Prefer `BigInt` literals over `BigInt(…)` constructor.",
    remediation: "Replace `BigInt(123)` with `123n` — the literal form is shorter and clearer.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
