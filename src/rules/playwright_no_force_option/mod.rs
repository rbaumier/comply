//! playwright-no-force-option — flag `{ force: true }` on Playwright actions.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-force-option",
    description: "`force: true` bypasses Playwright's actionability checks, hiding real UI issues.",
    remediation: "Remove `force: true` from the action options. If the \
                  element is not actionable, fix the underlying page state \
                  instead of bypassing the check — forcing clicks masks \
                  real accessibility and timing bugs.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
