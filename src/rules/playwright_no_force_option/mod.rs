//! playwright-no-force-option — flag `{ force: true }` on Playwright actions.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

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
    crate::register_ts_family!(META, typescript)
}
