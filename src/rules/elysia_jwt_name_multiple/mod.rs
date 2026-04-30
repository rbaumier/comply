//! elysia-jwt-name-multiple

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-jwt-name-multiple",
    description: "Multiple `jwt(...)` plugins registered without distinct `name` values — they overwrite each other.",
    remediation: "Pass a unique `name` to each `jwt({...})` call (e.g. `name: 'access'`, `name: 'refresh'`) so they register as separate decorators.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
