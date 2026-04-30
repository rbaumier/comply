//! elysia-cf-env-import

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-cf-env-import",
    description: "`process.env` is undefined in Cloudflare Workers — Elysia code under `CloudflareAdapter` must read env from the `cloudflare:workers` import.",
    remediation: "Replace `process.env.X` with `import { env } from 'cloudflare:workers'` and `env.X`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
