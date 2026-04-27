//! hono-no-unvalidated-body — flag `c.req.json()` / `c.req.parseBody()` without validation.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "hono-no-unvalidated-body",
    description: "Reading the request body without a validator middleware skips schema validation and can let malformed input reach handlers.",
    remediation: "Use `validator('json', schema)` (or `zValidator`, `tbValidator`, etc.) and read the parsed body via `c.req.valid('json')`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["hono", "security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
