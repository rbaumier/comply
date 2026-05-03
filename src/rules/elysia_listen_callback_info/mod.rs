//! elysia-listen-callback-info

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-listen-callback-info",
    description: "`.listen(PORT)` called without a callback — server boot info is silently dropped.",
    remediation: "Pass a callback to `.listen` and log `app.server?.hostname`/`app.server?.port` so deploys surface where the server is actually bound.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["observability", "elysia"],
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
