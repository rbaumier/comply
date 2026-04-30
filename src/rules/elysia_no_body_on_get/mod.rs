//! elysia-no-body-on-get

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-no-body-on-get",
    description: "`.get()` / `.head()` route declares a `body:` schema, which HTTP forbids.",
    remediation: "Move the validation to `query:` or change the verb to `.post()`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
