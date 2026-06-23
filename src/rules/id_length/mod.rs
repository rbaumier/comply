//! id-length — native implementation that names the offending identifier.
//!
//! Replaces the previous oxlint `id-length` + clippy `min_ident_chars`
//! delegation because the upstream diagnostics hid the actual
//! identifier behind a generic message (`Identifier name is too short
//! (< 2)`). Our version walks the tree-sitter AST for TS/JS/TSX and
//! Rust, only flags *binding* positions (declarations, not usages),
//! and emits `` Identifier `t` is too short (< 2) `` so a reader sees
//! the culprit without opening the file.
//!
//! Options are read from `[rules.id-length]` in `comply.toml`:
//!   - `min` (default `2`)
//!   - `exceptions` — exact-match allowlist (e.g. `["t", "T"]`)
//!   - `exception_patterns` — regex allowlist (e.g. `["^[A-Z]$"]`)

mod oxc_typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "id-length",
    description: "Identifier names shorter than `min` hide intent.",
    remediation: "Rename to a full word — `createdAt` not `d`, `userCount` \
                  not `n`. Allow-list conventional short names in \
                  `comply.toml`:\n\n\
                  [rules.id-length]\n\
                  exceptions = [\"t\", \"i\", \"j\"]\n\
                  exception_patterns = [\"^[A-Z]$\"]",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

/// True for a single-character identifier whose sole character is non-ASCII —
/// e.g. a Greek letter (`α`, `β`, `γ`). A non-ASCII single-char name is never a
/// lazy keyboard default; it is a deliberate mathematical/scientific notation
/// choice that already carries unambiguous meaning, which is exactly what the
/// rule's length floor exists to guarantee. This is a Unicode property of the
/// identifier, not a name allowlist; ASCII single chars (`q`, `_`, `$`) are
/// unaffected.
fn is_non_ascii_single_char(name: &str) -> bool {
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => !c.is_ascii(),
        _ => false,
    }
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
