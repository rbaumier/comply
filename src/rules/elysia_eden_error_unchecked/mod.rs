//! elysia-eden-error-unchecked

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-eden-error-unchecked",
    description: "Eden treaty calls return `{ data, error }` — destructuring only `data` swallows the error path.",
    remediation: "Destructure both `{ data, error }` and check `error` before consuming `data`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
