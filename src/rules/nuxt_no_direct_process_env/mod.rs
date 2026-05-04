//! nuxt-no-direct-process-env

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
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
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
