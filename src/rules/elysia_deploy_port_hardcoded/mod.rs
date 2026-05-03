//! elysia-deploy-port-hardcoded

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-deploy-port-hardcoded",
    description: "Elysia `.listen()` uses a hardcoded numeric port — deployment platforms typically inject the port via environment.",
    remediation: "Read the port from `process.env.PORT` (with a sensible default) so the same image works locally and on hosting.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["deployment", "elysia"],
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
