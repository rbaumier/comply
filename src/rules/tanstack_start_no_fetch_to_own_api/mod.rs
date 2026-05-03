//! tanstack-start-no-fetch-to-own-api — forbid `fetch('/api/...')` when a
//! `createServerFn` equivalent is the preferred transport.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-no-fetch-to-own-api",
    description: "Don't `fetch('/api/...')` your own app; call a server function.",
    remediation: "Replace in-app `fetch('/api/...')` calls with a typed \
                  `createServerFn` call — you gain type safety and skip the \
                  HTTP round-trip on SSR.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-start"],
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
