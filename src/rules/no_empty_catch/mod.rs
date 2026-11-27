//! no-empty-catch — flag silently swallowed errors.
//! Covers empty catch blocks and discarded promise rejection handlers.

mod oxc_typescript;
mod rust;
#[cfg(test)]
mod tests;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-empty-catch",
    description: "Disallow empty catch blocks and discarded promise rejection handlers \
                  that silently swallow errors.",
    remediation: "Handle the error explicitly — log it, rethrow, or add a comment \
                  explaining why swallowing is intentional.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],

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
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}
