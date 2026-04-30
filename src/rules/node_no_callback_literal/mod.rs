//! node-no-callback-literal

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "node-no-callback-literal",
    description: "First argument to error-first callbacks should be an Error object or `null`, not a string literal.",
    remediation: "Pass `new Error('...')` or `null` as the first argument instead of a string literal.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["node"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
