//! no-duplicated-branches — flag if/else or match branches with identical
//! bodies.
//!
//! ## Pattern-binding mode (Rust only)
//!
//! When a Rust `if`/`else if` chain contains at least one `let_condition`
//! (`if let PAT = EXPR`), the branch key becomes `(condition, body)`
//! instead of `body` alone. `if let` introduces pattern-bound names that
//! live only in the corresponding branch, so two branches can share an
//! identical body text while each reference a distinct binding. A flat
//! body-text comparison would flag these as duplicates — they are not.
//!
//! Two `if let` branches with textually identical conditions AND bodies
//! remain a real duplicate and stay flagged.
//!
//! Match arms stay on body-only comparison: OR-patterns (`A(n) | B(n)`)
//! can merge two arms with different patterns and identical bodies, which
//! is the refactoring the rule is meant to suggest.
//!
//! ## Dedup
//!
//! Each duplicate line is reported at most once per chain. The previous
//! implementation used an O(n²) pairwise loop that reported line `j` once
//! per earlier match, emitting three diagnostics on a three-branch
//! repeat.

mod oxc_typescript;
mod rust;
#[cfg(test)]
mod shared_tests;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-duplicated-branches",
    description: "Two branches of an if/else have identical bodies.",
    remediation: "Merge the conditions or remove the duplicate branch.",
    severity: Severity::Warning,
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
