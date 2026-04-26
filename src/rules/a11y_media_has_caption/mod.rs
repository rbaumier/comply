//! a11y-media-has-caption

mod vue;
mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::files::Language;
use crate::rules::backend::Backend;
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
    let mut backends = crate::register_ts_family!(META, react).backends;
    backends.push((Language::Vue, Backend::Text(Box::new(vue::Check))));
    RuleDef { meta: META, backends }
}
