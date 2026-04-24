//! ts-no-mixed-sync-async-returns — forbid T | Promise<T> return types.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-mixed-sync-async-returns",
    description: "Functions must not conditionally return sync or Promise values — pick one.",
    remediation: "Make the function `async` so it always returns a Promise, or extract the sync path to a separate function.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
