//! rust-prefer-once-lock — `lazy_static!` and `once_cell` are obsolete.
//!
//! Since Rust 1.70, `std::sync::OnceLock` and `std::sync::LazyLock`
//! cover the same use cases as `lazy_static!` and `once_cell::sync::{Lazy,OnceCell}`
//! without a third-party dep, heavy macros, or init-order pitfalls.
//! Flag `lazy_static!` and `once_cell::sync::{Lazy,OnceCell}` so new
//! code picks the std primitive.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-prefer-once-lock",
    description: "`lazy_static!` and `once_cell` are superseded by `std::sync::OnceLock`/`LazyLock` (Rust 1.70+).",
    remediation: "Replace `lazy_static! { static ref X: T = ... }` with `static X: LazyLock<T> = LazyLock::new(|| ...);`.",
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
