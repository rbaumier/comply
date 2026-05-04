//! elysia-route-all-method

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-route-all-method",
    description: "`.all()` accepts any HTTP method — usually a specific method is more appropriate.",
    remediation: "Replace `.all('/path', ...)` with `.get`, `.post`, `.put`, `.patch`, or `.delete` to communicate intent and let routers/proxies cache safely.",
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
