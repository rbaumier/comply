//! rust-arc-non-send-sync — Arc around `!Send` or `!Sync` types is wrong.
//!
//! Doc-only marker rule. The actual enforcement lives in clippy:
//! `clippy::arc_with_non_send_sync` is in the `correctness` group
//! and warns by default. comply registers the rule so it shows up
//! in `comply list` / `comply explain` alongside the rest of the
//! Rust catalog, but does not run clippy itself.
//!
//! Example failure: `Arc<RefCell<T>>` — `RefCell` is `!Sync`, so the
//! `Arc` cannot be sent across threads. Either you don't need the
//! `Arc` (use plain `Rc<RefCell<T>>`) or you need a thread-safe
//! interior (`Arc<Mutex<T>>`).

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-arc-non-send-sync",
    description: "`Arc<T>` where `T: !Send + !Sync` cannot cross threads.",
    remediation: "Either drop the `Arc` (use `Rc<T>` for single-threaded \
                  sharing) or replace the inner type with a thread-safe \
                  one — `Arc<RefCell<T>>` → `Arc<Mutex<T>>`. Enforced by \
                  `clippy::arc_with_non_send_sync` (correctness, on by default).",
    severity: Severity::Error,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![],
    }
}
