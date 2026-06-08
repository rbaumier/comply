//! rust-no-bool-return-from-fallible — actions return `Result`, not `bool`.
//!
//! Functions that perform an action — `save`, `delete`, `parse`,
//! `validate`, `connect`, `send` — must return `Result<T, E>` so the
//! caller can see WHY the operation failed. A bare `bool` collapses
//! every failure mode into "true or false" and forces the caller to
//! either give up or run a separate diagnostic call.
//!
//! Pure predicate functions (`is_empty`, `has_field`, `contains`) are
//! NOT covered by this rule — boolean is the right return type for
//! questions about state.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-bool-return-from-fallible",
    description: "Action functions return `Result`, not `bool`.",
    remediation: "Change the return type to `Result<T, E>` (use `()` for \
                  T if there's no payload). A bool tells the caller \
                  something failed but not why — they can't handle the \
                  error specifically, only choose to give up or retry blindly.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};
pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
