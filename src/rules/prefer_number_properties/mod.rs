//! prefer-number-properties — prefer `Number` static properties over globals.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-number-properties",
    description: "Prefer `Number.isNaN()`, `Number.parseInt()`, etc. over global equivalents.",
    remediation: "Replace global `isNaN()`, `isFinite()`, `parseInt()`, `parseFloat()`, `NaN`, \
                  and `Infinity` with their `Number.*` equivalents. The `Number` methods are \
                  stricter (no implicit coercion) and the properties are unambiguous.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
