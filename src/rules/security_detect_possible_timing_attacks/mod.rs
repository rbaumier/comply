//! security-detect-possible-timing-attacks — `==` on secrets / passwords.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "security-detect-possible-timing-attacks",
    description: "String equality on a secret / password / token leaks length and partial bytes via timing.",
    remediation: "Use a constant-time compare: Node's `crypto.timingSafeEqual(Buffer.from(a), Buffer.from(b))`, or a library like `safe-compare`.",
    severity: Severity::Error,
    doc_url: Some("https://github.com/eslint-community/eslint-plugin-security/blob/main/docs/rules/detect-possible-timing-attacks.md"),
    categories: &["security"],
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
