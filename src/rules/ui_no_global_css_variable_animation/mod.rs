//! ui-no-global-css-variable-animation — `document.documentElement.style.setProperty`
//! inside `requestAnimationFrame` triggers full-page style recalc every frame.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-global-css-variable-animation",
    description: "Global CSS variable change inside `requestAnimationFrame` triggers full-page recalc.",
    remediation: "Scope the CSS variable to the animated element: \
                  `element.style.setProperty('--x', value)` instead of \
                  `document.documentElement.style.setProperty('--x', value)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
