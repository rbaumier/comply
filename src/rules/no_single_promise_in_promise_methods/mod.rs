//! no-single-promise-in-promise-methods — flag `Promise.all([single])`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-single-promise-in-promise-methods",
    description: "Wrapping a single-element array with `Promise.all/any/race()` is unnecessary.",
    remediation: "Use the value directly instead of wrapping it in a Promise method: \
                  `await single` instead of `await Promise.all([single])`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
