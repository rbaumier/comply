mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "better-result-no-promise-catch",
    description: "Replace .catch() on Promise with Result.tryPromise() in better-result modules.",
    remediation: "Wrap the promise with Result.tryPromise({ try, catch }) instead of chaining .catch().",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["better-result"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
