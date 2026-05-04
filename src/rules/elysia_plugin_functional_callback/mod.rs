//! elysia-plugin-functional-callback

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-plugin-functional-callback",
    description: "Functional plugins `(app: Elysia) => app.get(...)` lose type inference — prefer `new Elysia()` instances.",
    remediation: "Export `new Elysia({ name }).get(...)` and `.use(plugin)` instead of an arrow that mutates the parent app.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "elysia"],
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
