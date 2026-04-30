//! a11y-button-without-accessible-name — flag `<button>` elements
//! whose only children are SVG / icon components (no readable text)
//! and that lack `aria-label` / `aria-labelledby` / `title`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-button-without-accessible-name",
    description: "Icon-only `<button>` without `aria-label` is unannounceable to screen readers.",
    remediation: "Add `aria-label`, `aria-labelledby`, or visible text content.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
