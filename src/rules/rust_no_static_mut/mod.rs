//! rust-no-static-mut — `static mut` is deprecated in Rust 2024.
//!
//! `static mut` is the most error-prone construct in safe Rust:
//! every read/write requires `unsafe`, data races are trivial, and
//! the Rust 2024 edition deprecates the feature entirely. Use a
//! safe synchronization primitive (`OnceLock`, `LazyLock`, `Mutex`,
//! `RwLock`, atomics) instead.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

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
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};
pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
