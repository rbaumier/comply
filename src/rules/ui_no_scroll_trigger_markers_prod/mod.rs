//! ui-no-scroll-trigger-markers-prod — GSAP `ScrollTrigger` `markers: true`
//! must be guarded by `process.env.NODE_ENV` to avoid shipping debug UI.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-scroll-trigger-markers-prod",
    description: "`markers: true` in a ScrollTrigger config ships debug overlays to production.",
    remediation: "Gate it: `markers: process.env.NODE_ENV !== \"production\"`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
