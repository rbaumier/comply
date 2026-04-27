//! nuxt-no-direct-process-env

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "nuxt-no-direct-process-env",
    description: "`process.env` is unavailable on the client and bypasses Nuxt's runtime config.",
    remediation: "Use `useRuntimeConfig()` to read both public and private runtime values.",
    severity: Severity::Error,
    doc_url: Some("https://nuxt.com/docs/guide/going-further/runtime-config"),
    categories: &["nuxt"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
