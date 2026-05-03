//! Prefer static regex outside functions.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-static-regex",
    description: "Regex literals inside functions are recompiled on each call.",
    remediation: "Hoist the regex to module scope or use a constant.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/nicolo-ribaudo/eslint-plugin-e18e"),
    categories: &["performance"],
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
