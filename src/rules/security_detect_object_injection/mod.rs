//! security-detect-object-injection — bracket access with non-literal key.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "security-detect-object-injection",
    description: "Bracket access `obj[expr]` where `expr` comes from untrusted input enables prototype pollution and data exfiltration.",
    remediation: "Validate the key against an allowlist before indexing, or use `Map`/`Set` which don't have a prototype chain. For static lookups, use a `switch` or `Object.hasOwn(obj, key)` guarded access.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/eslint-community/eslint-plugin-security/blob/main/docs/rules/detect-object-injection.md"),
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
