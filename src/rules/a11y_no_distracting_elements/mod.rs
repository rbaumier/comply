//! a11y-no-distracting-elements

mod react;
mod vue;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-no-distracting-elements",
    description: "Flag `<marquee>` and `<blink>` elements which are distracting and deprecated.",
    remediation: "Remove `<marquee>` and `<blink>` elements. Use CSS animations if motion is needed, with `prefers-reduced-motion` support.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    let mut backends = crate::register_ts_family!(META, react).backends;
    backends.push((Language::Vue, Backend::Text(Box::new(vue::Check))));
    RuleDef {
        meta: META,
        backends,
    }
}
