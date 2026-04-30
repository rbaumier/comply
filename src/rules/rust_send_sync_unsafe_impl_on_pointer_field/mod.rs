//! rust-send-sync-unsafe-impl-on-pointer-field — `unsafe impl Send for X` /
//! `unsafe impl Sync for X` is a hand-waved promise of thread safety. When
//! `X` contains raw pointers, `Cell`, or `RefCell` fields the promise is
//! almost always wrong — those types are explicitly `!Send` / `!Sync` for
//! sound reasons.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-send-sync-unsafe-impl-on-pointer-field",
    description: "`unsafe impl Send/Sync` on a struct with raw-pointer or `Cell`/`RefCell` fields.",
    remediation: "Don't hand-wave thread safety. Either use a sync wrapper \
                  (`Mutex`, `Atomic*`, `parking_lot::Mutex`) so the auto \
                  trait derives correctly, or wrap the raw pointer in \
                  `NonNull<T>` + an explicit `// SAFETY:` comment that \
                  argues why the access pattern is sound. Replace `Cell` \
                  with an `AtomicCell` or `Mutex<T>` for cross-thread use.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust", "concurrency", "unsafe"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
