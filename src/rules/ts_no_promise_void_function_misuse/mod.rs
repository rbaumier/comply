//! ts-no-promise-void-function-misuse — passing an `async` callback to a
//! function that ignores its return value (`setTimeout`, `setInterval`,
//! `.forEach`) silently swallows promise rejections.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-promise-void-function-misuse",
    description: "Async callback passed to a void-return slot — rejections become unhandled.",
    remediation: "Wrap the callback: `setTimeout(() => { void asyncFn(); }, 100)`. \
                  For `.forEach`, switch to `for ... of` with `await` or `Promise.all(arr.map(async ...))`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "async"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
