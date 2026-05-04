//! elysia-cors-allowed-headers-wildcard

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-cors-allowed-headers-wildcard",
    description: "`cors({ credentials: true })` with wildcard or omitted `allowedHeaders` — browsers reject the preflight.",
    remediation: "List the explicit headers your API accepts (e.g. `allowedHeaders: ['content-type', 'authorization']`). Wildcards are invalid when `credentials: true`.",
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
