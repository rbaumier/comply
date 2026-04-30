//! elysia-heavy-onrequest

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-heavy-onrequest",
    description: "`.onRequest()` performs heavy work (await/fetch/db/JSON.parse) — runs before routing.",
    remediation: "Move heavy work to `.beforeHandle()` (per route) or `.derive()`/`.resolve()` so it runs only for routes that need it.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
