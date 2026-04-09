//! rust-await-holding-lock — never hold a lock across .await.

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-await-holding-lock",
    description: "Never hold a MutexGuard across an `.await` point.",
    remediation: "Drop the guard before awaiting: copy the needed data out \
                  in a tight scope, `drop(guard)`, then await. Locks held \
                  across awaits cause deadlocks under tokio's scheduler. \
                  Enable `clippy::await_holding_lock`.",
    severity: Severity::Error,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![],
    }
}
