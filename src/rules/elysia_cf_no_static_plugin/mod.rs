//! elysia-cf-no-static-plugin

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-cf-no-static-plugin",
    description: "Elysia `staticPlugin` / `.file()` are unsupported under the Cloudflare adapter — there is no filesystem on Workers.",
    remediation: "Serve static assets via Cloudflare's `[assets]` binding (Workers Sites / Static Assets) instead.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
