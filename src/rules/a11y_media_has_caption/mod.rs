//! a11y-media-has-caption

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-media-has-caption",
    description: "Flag `<video>` and `<audio>` elements without `<track kind=\"captions\">` children.",
    remediation: "Add a `<track kind=\"captions\" src=\"...\" />` element inside `<video>` or `<audio>` to provide captions.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
