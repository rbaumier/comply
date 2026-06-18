//! prefer-less-than — suggest rewriting `b > a` as `a < b` for readability.

mod oxc_typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-less-than",
    description: "Prefer `<` / `<=` over `>` / `>=` for readability.",
    remediation: "Prefer `<` over `>` for readability",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

/// True for SCREAMING_SNAKE_CASE / all-uppercase identifiers (named constants):
/// at least one letter, every letter uppercase, only letters/digits/underscores.
/// Shared by both backends to classify a comparison's left operand.
fn is_screaming_snake_case(name: &str) -> bool {
    let mut has_letter = false;
    for ch in name.chars() {
        if ch.is_alphabetic() {
            if ch.is_lowercase() {
                return false;
            }
            has_letter = true;
        } else if !ch.is_ascii_digit() && ch != '_' {
            return false;
        }
    }
    has_letter
}

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
