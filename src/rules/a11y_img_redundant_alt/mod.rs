//! a11y-img-redundant-alt

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-img-redundant-alt",
    description: "`alt` text should not contain redundant words like \"image\", \"picture\", or \"photo\".",
    remediation: "Describe the image content instead of stating that it is an image. Remove words like \"image\", \"picture\", or \"photo\" from the `alt` attribute.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
