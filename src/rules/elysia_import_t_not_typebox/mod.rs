//! elysia-import-t-not-typebox

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-import-t-not-typebox",
    description: "TypeBox imported directly in an Elysia file — Elysia's `t` is the augmented public surface.",
    remediation: "Import `t` from `elysia` instead of `Type` from `@sinclair/typebox` — Elysia's `t` includes augmented validators.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["type-safety", "elysia"],
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
