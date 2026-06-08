//! rust-match-lock-guard-scrutinee — `match mtx.lock() { … }` keeps the
//! lock held through every arm because the temporary `MutexGuard` lives
//! until the end of the `match` expression. Bind the locked value into a
//! shorter scope first.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-match-lock-guard-scrutinee",
    description: "`match` scrutinee that locks holds the guard across every arm.",
    remediation: "Pull the lock out: `let guard = mtx.lock().unwrap(); \
                  let value = guard.field.clone(); drop(guard); match \
                  value { … }`. The `match` form holds the guard until \
                  the entire `match` expression finishes, so any arm \
                  that takes another lock can deadlock.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust", "concurrency"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
