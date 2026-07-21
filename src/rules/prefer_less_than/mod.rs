//! prefer-less-than — flag Yoda-style `>` / `>=` comparisons, where the left
//! operand is strictly more constant-like than the right, and suggest the
//! swapped `<` / `<=` form.

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
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

/// How constant-like a comparison operand reads. Swapping the operands of
/// `>` / `>=` only improves readability when the left side ranks strictly
/// higher than the right: `5 > x` reads worse than `x < 5`, whereas swapping
/// `MAX > 0` would *create* the Yoda condition the rule exists to remove.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd)]
enum Constness {
    /// A variable, field, call, dereference, or any other runtime-computed
    /// value — the natural subject of a comparison.
    Subject,
    /// A named constant: a SCREAMING_SNAKE_CASE identifier, or the final
    /// segment of a path (Rust) or of a member access (TS/JS).
    NamedConstant,
    /// A literal value written inline.
    Literal,
}

/// Rank an operand that is a bare name (identifier, path tail, property).
fn name_constness(name: &str) -> Constness {
    if is_screaming_snake_case(name) {
        Constness::NamedConstant
    } else {
        Constness::Subject
    }
}

/// True for SCREAMING_SNAKE_CASE / all-uppercase identifiers (named constants):
/// at least one letter, every letter uppercase, only letters/digits/underscores.
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
