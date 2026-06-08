//! security-detect-insecure-randomness — `Math.random()` in security context.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "security-detect-insecure-randomness",
    description: "`Math.random()` is not cryptographically secure — using it to generate tokens, session ids, passwords or keys is exploitable.",
    remediation: "Use `crypto.randomUUID()` / `crypto.getRandomValues(...)` / `crypto.randomBytes(...)`. For passwords specifically, use a KDF library.",
    severity: Severity::Error,
    doc_url: Some("https://github.com/eslint-community/eslint-plugin-security/blob/main/docs/rules/detect-pseudoRandomBytes.md"),
    categories: &["security"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
