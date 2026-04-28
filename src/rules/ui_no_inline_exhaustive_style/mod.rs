//! ui-no-inline-exhaustive-style — inline `style={{...}}` with more than 8
//! properties should be extracted to a CSS class or styled component.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-inline-exhaustive-style",
    description: "Inline `style` object with too many properties — extract to a CSS class.",
    remediation: "Move the styles to a CSS module, Tailwind classes, or a styled component. \
                  Inline styles with many properties hurt readability and prevent reuse.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
