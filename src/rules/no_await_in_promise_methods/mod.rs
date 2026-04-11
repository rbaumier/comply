//! no-await-in-promise-methods — flag `await` inside `Promise.all/race/any/allSettled` arrays.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-await-in-promise-methods",
    description: "Promise in `Promise.all/race/any/allSettled()` should not be awaited.",
    remediation: "Remove the `await` keyword from array elements passed to Promise methods. \
                  Awaiting inside the array serializes the calls, defeating the purpose of \
                  `Promise.all()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
