//! elysia-static-inline-value

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-static-inline-value",
    description: "Route handler returns only a static string literal — pass the literal directly for ahead-of-time response caching.",
    remediation: "Replace `.get('/health', () => 'ok')` with `.get('/health', 'ok')`. Elysia compiles literal responses ahead of time.",
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
