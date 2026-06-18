mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-no-duplicate-secret",
    description: "Avoid hardcoding `secret` in `betterAuth()` — rely on `BETTER_AUTH_SECRET`.",
    remediation: "Remove `secret` from the config and set `BETTER_AUTH_SECRET` in the environment instead.",
    severity: Severity::Warning,
    doc_url: Some("https://www.better-auth.com/docs/installation"),
    categories: &["better-auth"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
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
