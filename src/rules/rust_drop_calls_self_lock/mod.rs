//! rust-drop-calls-self-lock — `Drop::drop` body acquires a lock on `self`.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-drop-calls-self-lock",
    description: "`Drop::drop` body calls `.lock()` / `.read()` / `.write()` \
                  on a `self.field`.",
    remediation: "Acquiring a lock during `Drop` deadlocks if the same lock \
                  is already held on the dropping thread, and risks panic if \
                  the lock is poisoned. Restructure so locking happens \
                  before drop, or store data in a way that doesn't require \
                  re-locking on cleanup.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
