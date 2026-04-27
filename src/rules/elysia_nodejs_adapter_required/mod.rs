//! elysia-nodejs-adapter-required

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-nodejs-adapter-required",
    description: "Elysia under Node.js requires the explicit `@elysiajs/node` adapter — without it the runtime falls back to Bun-only APIs.",
    remediation: "Pass `adapter: node()` to the `Elysia` constructor when running on Node.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
