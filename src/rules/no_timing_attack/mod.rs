//! no-timing-attack — flag direct `==` / `!=` / `===` / `!==` comparison
//! of a value whose identifier name ends with a sensitive word
//! (`password`, `passwd`, `secret`, `apikey`, `auth`, `hash`, `digest`,
//! `hmac`, `credential`, `otp`, `pin`), or with an ambiguous role word
//! (`token`, `signature`) when the name also carries a secret indicator.
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
//! A name is sensitive when, after normalization (lowercase + strip `_`
//! so snake_case / camelCase / UPPER_SNAKE collapse to the same form),
//! it ends with a secret word. `token` and `signature` also name
//! non-security concepts (lexer / comment-syntax tokens, LSP
//! function-call signatures), so a name ending with one of those is only
//! sensitive when it also contains a secret indicator (`auth`, `access`,
//! `api`, …): `auth_token` is flagged, `comment_token` is not.
//!
//! A comparison inside the `eq` method of an `impl PartialEq for T` (Rust) is
//! exempt: `self.hash == other.hash` there is a structural-hash short-circuit
//! over two fields of the same `&Self`, with no attacker-input vs. stored-secret
//! asymmetry, so the timing-attack premise does not apply.
//!
//! A comparison where either operand is a JS `Symbol` (TS) is exempt: an inline
//! `Symbol(...)` / `Symbol.for(...)` call, or an identifier bound to one. A
//! `Symbol` is compared by reference identity (an O(1) id check), not byte by
//! byte, so the capability-token idiom (`const secret = Symbol(); arg ===
//! secret`) cannot leak timing.
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

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
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
