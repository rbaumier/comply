//! rust-if-let-mutex-lock — `if let ... = mtx.lock()` keeps lock alive in else.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-if-let-mutex-lock",
    description: "`if let ... = mtx.lock()` extends the lock guard's lifetime \
                  to cover the `else` branch.",
    remediation: "Temporaries created in the scrutinee of `if let` live for \
                  the entire `if/else` expression. Hold the guard in a \
                  separate `let` binding so the lock is released before the \
                  `else` branch runs, or use `match` with explicit `drop()` calls.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
