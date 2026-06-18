mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-no-disable-csrf",
    description: "`disableCSRFCheck: true` removes CSRF protection from Better Auth.",
    remediation: "Remove `disableCSRFCheck` — CSRF protection is enabled by default and must stay on.",
    severity: Severity::Error,
    doc_url: Some("https://www.better-auth.com/docs/security"),
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
