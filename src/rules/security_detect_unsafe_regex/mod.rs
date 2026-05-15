//! security-detect-unsafe-regex — catastrophic backtracking (ReDoS).

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "security-detect-unsafe-regex",
    description: "Regular expression vulnerable to catastrophic backtracking (ReDoS).",
    remediation: "Rewrite the pattern without nested quantifiers and avoid `(a+)+`-style \"evil regex\" shapes. Consider using a safe regex engine (rust regex crate, RE2) or pre-validate input length.",
    severity: Severity::Error,
    doc_url: Some("https://github.com/eslint-community/eslint-plugin-security/blob/main/docs/rules/detect-unsafe-regex.md"),
    categories: &["security"],
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
