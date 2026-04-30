//! ui-no-tiny-text — inline `fontSize` numeric values below 12 (pixels) are
//! too small for comfortable reading on most screens.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-tiny-text",
    description: "Inline `fontSize` below 12px — too small for comfortable reading.",
    remediation: "Use a fontSize of at least 12px, or rely on a typography scale defined in your \
                  design system.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
