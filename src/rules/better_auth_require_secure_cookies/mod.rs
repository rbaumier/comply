mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-require-secure-cookies",
    description: "Better Auth config missing `useSecureCookies: true` — session cookies transmitted over HTTP.",
    remediation: "Add `advanced: { useSecureCookies: true }` to your Better Auth config for production.",
    severity: Severity::Warning,
    doc_url: Some("https://www.better-auth.com/docs/concepts/cookies"),
    categories: &["security", "auth"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
