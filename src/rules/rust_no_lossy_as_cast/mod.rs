//! rust-no-lossy-as-cast — `as` truncates silently.
//!
//! `let n: u8 = some_u32 as u8` does what you'd expect when the
//! value fits and produces nonsense the moment it doesn't — no
//! panic, no error, `255 + 1 → 0`. The `try_into()` /
//! `u8::try_from()` path returns a `Result` and forces the caller
//! to think about the overflow case.
//!
//! Integer-narrowing casts are the most common source of silent
//! corruption, so the rule flags them. A float source cast to an
//! integer target (`x.floor() as i32`) is left alone: std has no
//! `From` / `TryFrom` from `f32`/`f64` to any integer, so `as` is
//! the only conversion and the `try_into()` remediation would not
//! compile.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

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
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};
pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
