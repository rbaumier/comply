//! a11y-no-distracting-elements

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
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
    crate::register_ts_family!(META, typescript)
}
