//! a11y-alt-text

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-alt-text",
    description: "`<img>`, `<area>`, and `<input type=\"image\">` must have an `alt` attribute.",
    remediation: "Add an `alt` attribute describing the image content, or `alt=\"\"` for decorative images.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
