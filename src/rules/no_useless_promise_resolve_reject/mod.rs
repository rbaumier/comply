//! no-useless-promise-resolve-reject

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-useless-promise-resolve-reject",
    description: "Disallow returning `Promise.resolve/reject()` in async functions.",
    remediation: "In an async function, `return value` already wraps in \
                  `Promise.resolve()` and `throw error` already wraps in \
                  `Promise.reject()`. Remove the unnecessary wrapper.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
