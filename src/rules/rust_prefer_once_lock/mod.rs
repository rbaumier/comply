//! rust-prefer-once-lock — `lazy_static!` and `once_cell` are obsolete.
//!
//! Since Rust 1.70, `std::sync::OnceLock` and `std::sync::LazyLock`
//! cover the same use cases as `lazy_static!` and `once_cell::sync::{Lazy,OnceCell}`
//! without a third-party dep, heavy macros, or init-order pitfalls.
//! Flag `lazy_static!` and `once_cell::sync::{Lazy,OnceCell}` so new
//! code picks the std primitive.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-prefer-once-lock",
    description: "`lazy_static!` and `once_cell` are superseded by `std::sync::OnceLock`/`LazyLock` (Rust 1.70+).",
    remediation: "Replace `lazy_static! { static ref X: T = ... }` with `static X: LazyLock<T> = LazyLock::new(|| ...);`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
