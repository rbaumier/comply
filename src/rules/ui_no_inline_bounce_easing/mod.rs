//! ui-no-inline-bounce-easing — bounce/elastic easing feels dated; real
//! objects decelerate smoothly.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-inline-bounce-easing",
    description: "Bounce/elastic easing in inline styles — use `ease-out` or a smooth deceleration curve.",
    remediation: "Replace bounce/elastic/wobble easing with `ease-out` or \
                  `cubic-bezier(0.16, 1, 0.3, 1)` for a modern feel.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
