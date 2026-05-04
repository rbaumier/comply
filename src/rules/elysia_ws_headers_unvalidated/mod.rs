//! elysia-ws-headers-unvalidated

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-ws-headers-unvalidated",
    description: "WebSocket route reads request headers in `beforeHandle` but does not declare a header schema.",
    remediation: "Add a `headers` (TypeBox) schema so Elysia validates the upgrade request headers before invoking `beforeHandle`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "elysia"],
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
