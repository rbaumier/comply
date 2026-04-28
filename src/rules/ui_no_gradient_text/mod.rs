//! ui-no-gradient-text — `background-clip: text` with a gradient background
//! creates gradient text that is often inaccessible and hard to read.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-gradient-text",
    description: "Gradient text (`background-clip: text` + gradient) is hard to read and often inaccessible.",
    remediation: "Use a solid text color for readability. If the gradient is \
                  essential for branding, ensure WCAG contrast ratio is met.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
