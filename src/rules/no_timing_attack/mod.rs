//! no-timing-attack — flag direct `==` / `!=` / `===` / `!==` comparison
//! of a value whose identifier name ends with a sensitive word
//! (`password`, `passwd`, `secret`, `token`, `apikey`, `auth`, `hash`,
//! `digest`, `signature`, `hmac`, `credential`, `otp`, `pin`).
//!
//! ## Why
//!
//! The built-in equality operators short-circuit on the first byte
//! mismatch, so an attacker who can measure response timing can brute-
//! force a secret one byte at a time (the classic string-comparison
//! timing attack). Use a constant-time comparison instead:
//! `constant_time_eq::constant_time_eq` or `subtle::ConstantTimeEq` in
//! Rust, `crypto.timingSafeEqual` in Node.js.
//!
//! ## Detection shape
//!
//! Walk `binary_expression` nodes. For each, inspect both operands:
//! - `identifier` → check the identifier text.
//! - `field_expression` (Rust) / `member_expression` (TS) → check the
//!   trailing field / property name.
//! - Any other kind (string literal, call expression, block, index
//!   expression, scoped path, …) is ignored.
//!
//! If either inspected name has a sensitive suffix after normalization
//! (lowercase + strip `_` so snake_case / camelCase / UPPER_SNAKE
//! collapse to the same form), the comparison is flagged. The previous
//! line-based scanner searched for sensitive words anywhere on the
//! line, which false-positived on string literals containing node-kind
//! names (e.g. `"index_signature"`) and on identifiers whose suffix is
//! neutral (`token_type`, `hash_map_size`, `auth_flow`).
//!
//! ## Known gap
//!
//! No constant propagation. A comparison whose operand is the result
//! of a call expression is not inspected, so
//! `get_password() == user_input` is missed. Adding it would require a
//! lightweight taint-style walk which is out of scope for this rule.

mod helpers;
mod oxc_typescript;
mod rust;
#[cfg(test)]
mod shared_tests;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{Language, RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-timing-attack",
    description: "Direct string comparison of secrets (passwords, tokens, hashes) is vulnerable to timing attacks.",
    remediation: "Use a constant-time comparison function like `crypto.timingSafeEqual()` (Node.js) or `constant_time_eq::constant_time_eq` / `subtle::ConstantTimeEq` (Rust).",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    let mut backends: Vec<(Language, Backend)> = TS_FAMILY
        .iter()
        .map(|&lang| (lang, Backend::Oxc(Box::new(oxc_typescript::Check))))
        .collect();
    backends.push((Language::Rust, Backend::TreeSitter(Box::new(rust::Check))));
    RuleDef {
        meta: META,
        backends,
    }
}
