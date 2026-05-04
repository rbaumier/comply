//! nuxt-no-client-only-in-ssr

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "nuxt-no-client-only-in-ssr",
    description: "Browser globals (`window`, `document`, `localStorage`) crash on the server without a client guard.",
    remediation: "Wrap the access in `if (import.meta.client)` / `if (process.client)`, move it to `onMounted`, or use `<ClientOnly>`.",
    severity: Severity::Error,
    doc_url: Some("https://nuxt.com/docs/api/components/client-only"),
    categories: &["nuxt", "ssr"],
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
