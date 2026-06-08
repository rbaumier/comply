//! rust-rc-mutex — `Rc<Mutex<T>>` is nonsense.
//!
//! `Rc` is `!Send` — you can't move it to another thread. `Mutex`
//! exists to synchronize access across threads. So `Rc<Mutex<T>>`
//! pays the Mutex cost (atomic ops, poisoning, etc.) for exactly
//! zero benefit — the value can never cross a thread boundary.
//! Use `Rc<RefCell<T>>` for single-threaded interior mutability.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-rc-mutex",
    description: "`Rc<Mutex<T>>` pays the Mutex cost for zero benefit — Rc is !Send.",
    remediation: "Replace `Rc<Mutex<T>>` with `Rc<RefCell<T>>` (single-threaded \
                  interior mutability, no atomic ops). If you actually need \
                  cross-thread sharing, use `Arc<Mutex<T>>` instead.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};
pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
