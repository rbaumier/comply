//! rust-no-as-numeric-cast — ban every `as` cast to a numeric type.
//!
//! Stricter than `rust-no-lossy-as-cast`: flags widening casts too
//! (e.g. `u8 as u64`). Forces `From::from` / `TryFrom::try_from`,
//! which document intent and stay greppable as the code evolves.
//!
//! False positives (e.g. `*const u8 as usize` pointer casts) are
//! accepted — suppress with `// comply-ignore` on the offending line.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-as-numeric-cast",
    description: "Ban every `as` cast whose target is a numeric primitive.",
    remediation: "Replace `x as u64` with `u64::from(x)` for guaranteed-safe \
                  widening, or `u64::try_from(x)?` for fallible narrowing. \
                  The goal is to make every integer conversion explicit and \
                  searchable — `as` hides the intent.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
