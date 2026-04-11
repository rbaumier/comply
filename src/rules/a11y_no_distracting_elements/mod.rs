//! a11y-no-distracting-elements

mod text;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-no-distracting-elements",
    description: "Flag `<marquee>` and `<blink>` elements which are distracting and deprecated.",
    remediation: "Remove `<marquee>` and `<blink>` elements. Use CSS animations if motion is needed, with `prefers-reduced-motion` support.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    let mut backends = crate::register_ts_family!(META, typescript).backends;
    backends.push((Language::Vue, Backend::Text(Box::new(text::Check))));
    RuleDef { meta: META, backends }
}
