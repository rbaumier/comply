//! rust-no-static-mut — `static mut` is deprecated in Rust 2024.
//!
//! `static mut` is the most error-prone construct in safe Rust:
//! every read/write requires `unsafe`, data races are trivial, and
//! the Rust 2024 edition deprecates the feature entirely. Use a
//! safe synchronization primitive (`OnceLock`, `LazyLock`, `Mutex`,
//! `RwLock`, atomics) instead.

mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-static-mut",
    description: "`static mut` is deprecated and unsafe by design.",
    remediation: "Replace `static mut FOO: T = ...` with a safe \
                  primitive: `OnceLock<T>`/`LazyLock<T>` for \
                  initialize-once values, `Mutex<T>`/`RwLock<T>` for \
                  shared mutable state, or `AtomicU64`/`AtomicBool`/etc \
                  for primitive counters and flags.",
    severity: Severity::Error,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Rust, Backend::TreeSitter(Box::new(rust::Check)))],
    }
}
