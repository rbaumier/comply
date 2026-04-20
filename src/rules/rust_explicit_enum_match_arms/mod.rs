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
//!   variant — i.e. a path pattern such as `Foo::A`, `Some(x)`,
//!   `Direction::North`, or `Self::Foo`.
//!
//! A pattern is considered enum-like if its text contains `::`, or
//! if the leading identifier starts with an ASCII uppercase letter
//! (`Some`, `None`, `Ok`, `Err`, `Direction`, …). This intentionally
//! accepts false negatives (e.g. enum match with only integer-like
//! patterns) rather than false positives (flagging a `match` on
//! integers where `_` is genuinely necessary).
//!
//! Test contexts are exempted for consistency with `rust-no-unwrap`:
//! test code routinely writes compact wildcard matches for setup
//! without losing much safety if a variant is later added.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

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
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
