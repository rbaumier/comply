//! elysia-deploy-no-health

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-deploy-no-health",
    description: "Elysia server exposes `.listen()` without a `/health` endpoint — load balancers and orchestrators have no liveness signal.",
    remediation: "Add `.get('/health', () => ({ status: 'ok' }))` (or similar) so platforms can probe readiness.",
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
