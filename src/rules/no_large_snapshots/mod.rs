//! no-large-snapshots
//!
//! Flags `toMatchInlineSnapshot` calls whose snapshot body spans more
//! than N lines. Large snapshots are noisy, slow to review, and
//! usually indicate testing the wrong thing (serialised whole-object
//! state rather than a focused assertion). The threshold is
//! configurable under `[rules.no-large-snapshots] max_lines`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
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
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
