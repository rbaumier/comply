//! rust-vec-with-capacity — avoid Vec reallocations when size is known.
//!
//! `let mut v = Vec::new();` followed by pushes inside a `for` loop
//! forces the allocator through a log2(n) sequence of doublings.
//! `Vec::with_capacity(n)` allocates once — same final size, zero
//! intermediate reallocations. Flags the common local-followed-by-loop
//! pattern so the fix is mechanical.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-vec-with-capacity",
    description: "`Vec::new()` followed by a for-loop with `.push()` reallocates repeatedly. Use `Vec::with_capacity()` when the size is known.",
    remediation: "Replace `Vec::new()` with `Vec::with_capacity(source.len())` when iterating a collection of known length.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Rust, Backend::Text(Box::new(text::Check)))],
    }
}
