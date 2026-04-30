//! ui-no-disabled-zoom — `<meta name="viewport">` with `user-scalable=no` or
//! `maximum-scale=1` prevents pinch-to-zoom, an accessibility violation.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-disabled-zoom",
    description: "Viewport meta disables pinch-to-zoom — accessibility violation.",
    remediation: "Remove `user-scalable=no` and `maximum-scale=1` from the viewport \
                  meta tag. Users with low vision rely on zoom.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
