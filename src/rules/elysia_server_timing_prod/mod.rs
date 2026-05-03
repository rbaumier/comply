//! elysia-server-timing-prod

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-server-timing-prod",
    description: "`serverTiming({ enabled: true })` hardcodes the header on — exposing internal timings to every client.",
    remediation: "Tie `enabled` to `process.env.NODE_ENV !== 'production'` or another internal-only flag.",
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
