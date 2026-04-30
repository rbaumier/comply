//! elysia-string-format-email

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-string-format-email",
    description: "Schema field named `email` / `url` / `uri` uses bare `t.String()` without `format:` constraint.",
    remediation: "Pass `{ format: 'email' }` (or `'uri'`) so the schema rejects malformed values: `t.String({ format: 'email' })`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["validation", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
