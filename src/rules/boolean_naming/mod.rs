//! boolean-naming — booleans must start with a predicate prefix.

mod oxc_typescript;
mod rust;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "boolean-naming",
    description: "Boolean identifiers must start with is/has/should/can/will/did/was.",
    remediation: "Rename to convey the predicate: `ready` → `isReady` (TS) or \
                  `is_ready` (Rust). Use the positive form only — prefer \
                  `!isReady` over `isNotReady`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["naming"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}
