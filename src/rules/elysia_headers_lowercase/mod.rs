//! elysia-headers-lowercase

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-headers-lowercase",
    description: "`headers:` schema uses uppercase header names — runtime values are always lowercased.",
    remediation: "Use lowercase keys (`authorization`, `content-type`) — Elysia normalises incoming headers to lowercase before validation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
