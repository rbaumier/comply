//! elysia-cf-env-import

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-cf-env-import",
    description: "`process.env` is undefined in Cloudflare Workers — Elysia code under `CloudflareAdapter` must read env from the `cloudflare:workers` import.",
    remediation: "Replace `process.env.X` with `import { env } from 'cloudflare:workers'` and `env.X`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["correctness", "elysia"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
