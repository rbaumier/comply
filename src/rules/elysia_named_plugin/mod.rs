//! elysia-named-plugin

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-named-plugin",
    description: "Exported Elysia plugin instance has no `name` — deduplication and tracing degrade.",
    remediation: "Pass `new Elysia({ name: 'plugin-name' })` for plugins. Named plugins are deduplicated and surface in error traces.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance", "elysia"],
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
