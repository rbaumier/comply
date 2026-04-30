//! elysia-apollo-no-req-res

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-apollo-no-req-res",
    description: "Apollo with Elysia exposes `{ request }` in context — `req`/`res` come from the Express integration and are undefined here.",
    remediation: "Replace `context: ({ req, res }) => ...` with `context: ({ request }) => ...`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
