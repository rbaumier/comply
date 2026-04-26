//! filename-naming-convention

mod rust;
mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "filename-naming-convention",
    description: "Filename does not match the expected naming convention for its language.",
    remediation: "Use kebab-case for JS/TS/Vue filenames (e.g. `user-profile.ts`) and snake_case for Rust filenames (e.g. `user_profile.rs`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::Rust, Backend::Text(Box::new(rust::Check))),
            (Language::Vue, Backend::Text(Box::new(text::Check))),
        ],
    }
}
