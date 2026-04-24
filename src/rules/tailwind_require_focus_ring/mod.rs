//! tailwind-require-focus-ring — keyboard users need a visible focus
//! indicator on every interactive element. Require a `focus:ring-*` /
//! `focus-visible:ring-*` class on buttons, anchors, form controls, and
//! `role="button"` elements.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-require-focus-ring",
    description: "Interactive elements must carry a `focus:ring-*` class for keyboard a11y.",
    remediation: "Add `focus:ring-2` (and ideally `focus:ring-offset-2`, `focus:outline-none`) to buttons, anchors, inputs, selects, textareas, and role=button elements.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind", "a11y"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
