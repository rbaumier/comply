//! no-large-snapshots
//!
//! Flags `toMatchInlineSnapshot` calls whose snapshot body spans more
//! than N lines. Large snapshots are noisy, slow to review, and
//! usually indicate testing the wrong thing (serialised whole-object
//! state rather than a focused assertion). The threshold is
//! configurable under `[rules.no-large-snapshots] max_lines`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-large-snapshots",
    description: "Inline snapshots exceeding `max_lines` are noisy and signal over-broad assertions.",
    remediation: "Narrow the assertion to the field under test, or split into smaller snapshots.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance", "testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
