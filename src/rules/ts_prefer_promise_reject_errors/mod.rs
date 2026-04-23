//! ts-prefer-promise-reject-errors — require `Promise.reject()` to be called with an `Error`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-prefer-promise-reject-errors",
    description: "`Promise.reject()` should receive an `Error` instance, not a primitive or plain object.",
    remediation: "Call `Promise.reject(new Error('...'))` instead of passing a string, number, or object literal.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/prefer-promise-reject-errors/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
