//! ui-no-gray-on-colored-background — inline style with gray text `color`
//! and a saturated `backgroundColor` produces low-contrast text.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-gray-on-colored-background",
    description: "Gray text on colored background — low contrast, hard to read.",
    remediation: "Use a lighter or white text color on saturated backgrounds \
                  for adequate contrast.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
