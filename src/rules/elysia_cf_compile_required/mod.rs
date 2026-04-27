//! elysia-cf-compile-required

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-cf-compile-required",
    description: "Elysia under the Cloudflare adapter must call `.compile()` — the Workers runtime cannot tolerate the JIT path.",
    remediation: "Call `.compile()` on the Elysia instance before exporting it for Cloudflare.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
