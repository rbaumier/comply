//! elysia-derive-async-no-await

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-derive-async-no-await",
    description: "`.derive(async () => ...)` whose body never `await`s — the async wrapper makes the derived value a Promise that handlers must explicitly await.",
    remediation: "Either drop the `async` keyword or `await` the work inside `.derive(...)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
