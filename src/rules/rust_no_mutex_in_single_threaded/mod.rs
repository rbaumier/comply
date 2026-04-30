//! rust-no-mutex-in-single-threaded — `Mutex<T>` not inside `Arc<…>` is
//! almost never the right primitive. If the value isn't shared across
//! threads, `RefCell<T>` provides the same interior-mutability without
//! paying for atomic locking. If it is shared, it should live behind
//! `Arc<Mutex<T>>`.
//!
//! The rule fires on `Mutex<T>` type annotations that aren't wrapped in
//! a recognisable sharing primitive (`Arc`, `Rc`, `Lazy`, `OnceLock`,
//! `LazyLock`, `static`). Intentionally conservative — complex cases
//! (fields inside a struct that's itself shared) require manual audit.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-mutex-in-single-threaded",
    description: "`Mutex<T>` outside of `Arc<Mutex<T>>` is usually a `RefCell<T>` — no thread sharing means no reason to pay for a lock.",
    remediation: "Replace `Mutex<T>` with `RefCell<T>` when the value is not shared across threads, or wrap it in `Arc<Mutex<T>>` when it is.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust", "performance"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
