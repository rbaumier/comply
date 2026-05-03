//! Prefer `**` operator over `Math.pow()` (ES2016).

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-exponentiation-operator",
    description: "Prefer `x ** y` over `Math.pow(x, y)`.",
    remediation: "Replace `Math.pow(x, y)` with `x ** y` (ES2016).",
    severity: Severity::Warning,
    doc_url: Some("https://eslint.org/docs/latest/rules/prefer-exponentiation-operator"),
    categories: &["e18e", "modernization"],
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
