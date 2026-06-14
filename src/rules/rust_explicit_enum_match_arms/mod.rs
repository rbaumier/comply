//! rust-explicit-enum-match-arms — forbid wildcard `_` arms on enum `match`.
//!
//! Intent: when `match`ing on an enum, list every variant explicitly.
//! Adding a new variant to the enum should then cause a compile error
//! at the `match` site, forcing a conscious decision about the new
//! case instead of silently hitting a catch-all.
//!
//! Heuristic: tree-sitter does not do type resolution, so we cannot
//! know whether the scrutinee is an enum. Instead, we inspect the
//! arms of the same `match_expression`:
//!
//! - exactly one arm is a wildcard (`_`), AND
//! - at least one OTHER arm has a pattern that "looks like" an enum
//!   variant — i.e. a path pattern such as `Foo::A`,
//!   `Direction::North`, or `Self::Foo`.
//!
//! A pattern is considered enum-like if its text contains `::`, or
//! if it is a bare PascalCase identifier (an uppercase lead with at
//! least one lowercase letter, e.g. `Direction`, `Foo`). Range
//! patterns (`'a'..='z'`, `0..=9`) and SCREAMING_SNAKE_CASE constants
//! (`EOF_CHAR`, `NUL`) apply only to scalar types (`char`, integers,
//! bytes) and are never enum-like. This intentionally accepts false
//! negatives (e.g. enum match with only integer-like patterns)
//! rather than false positives (flagging a `match` on a scalar type
//! where `_` is genuinely necessary).
//!
//! Stdlib exemption: when every enum-like arm references a known
//! stdlib closed or non_exhaustive enum — `Result` (`Ok`/`Err`),
//! `Option` (`Some`/`None`), or `std::io::ErrorKind` — the match is
//! not flagged. All arms of a `match` share one type, so this is a
//! sound syntactic proxy for "the scrutinee is a stdlib type". The
//! wildcard is idiomatic on Result/Option (stable, closed, two
//! variants) and compiler-mandated on `#[non_exhaustive]` enums like
//! `ErrorKind`. Project-defined enums still require explicit arms.
//!
//! Variant-accessor exemption: a `_ => None` arm paired with at least one
//! `Variant(v) => Some(v)` arm is the idiomatic "extract this variant, else
//! nothing" accessor. A new variant should still return `None` here, so
//! exhaustive listing adds noise without safety — the wildcard is not flagged.
//!
//! Test contexts are exempted for consistency with `rust-no-unwrap`:
//! test code routinely writes compact wildcard matches for setup
//! without losing much safety if a variant is later added.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-explicit-enum-match-arms",
    description: "Wildcard `_` arm on a `match` that looks like it covers an enum.",
    remediation: "Replace `_ => …` with the remaining variants explicitly \
                  (`Foo::A | Foo::B => …`). The compile error on enum expansion \
                  is the whole point: it forces you to consciously handle each \
                  new case.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
