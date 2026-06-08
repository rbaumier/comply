//! rust-vec-with-capacity — avoid Vec reallocations when size is known.
//!
//! `let mut v = Vec::new();` followed by pushes inside a `for` loop
//! forces the allocator through a log2(n) sequence of doublings.
//! `Vec::with_capacity(n)` allocates once — same final size, zero
//! intermediate reallocations. Flags the common local-followed-by-loop
//! pattern so the fix is mechanical.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-vec-with-capacity",
    description: "`Vec::new()` followed by a for-loop with `.push()` reallocates repeatedly. Use `Vec::with_capacity()` when the size is known.",
    remediation: "Replace `Vec::new()` with `Vec::with_capacity(source.len())` when iterating a collection of known length.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
