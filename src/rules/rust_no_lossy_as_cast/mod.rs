//! rust-no-lossy-as-cast — `as` truncates silently.
//!
//! `let n: u8 = some_u32 as u8` does what you'd expect when the
//! value fits and produces nonsense the moment it doesn't — no
//! panic, no error, `255 + 1 → 0`. The `try_into()` /
//! `u8::try_from()` path returns a `Result` and forces the caller
//! to think about the overflow case.
//!
//! Floats-to-int and int-narrowing casts are the most common
//! sources of silent corruption. Float-to-int saturates in modern
//! Rust but still loses precision; the rule flags both.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-lossy-as-cast",
    description: "`as` casts that can truncate or lose precision are silent bugs.",
    remediation: "Replace the `as` cast with `try_into()` (returns Result) \
                  or `u8::try_from(x)` for integer narrowing. For \
                  guaranteed-safe widening casts (`u8` → `u32`), use \
                  `From::from(x)` / `x.into()` instead — explicit, \
                  documents the conversion is total.",
    severity: Severity::Warning,
    doc_url: None,
};pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
