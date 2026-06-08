mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-require-secure-cookies",
    description: "Better Auth config missing `useSecureCookies: true` — session cookies transmitted over HTTP.",
    remediation: "Add `advanced: { useSecureCookies: true }` to your Better Auth config for production.",
    severity: Severity::Warning,
    doc_url: Some("https://www.better-auth.com/docs/concepts/cookies"),
    categories: &["security", "better-auth"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
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
