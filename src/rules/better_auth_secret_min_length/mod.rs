mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-secret-min-length",
    description: "Better Auth `secret` must be at least 32 characters long.",
    remediation: "Use a secret of 32+ characters (e.g. generated via `openssl rand -base64 32`).",
    severity: Severity::Error,
    doc_url: Some("https://www.better-auth.com/docs/installation"),
    categories: &["better-auth", "security"],
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
