//! perf-no-render-blocking-css — a `<link rel="stylesheet">` without a
//! `media` attribute blocks first paint. Non-critical stylesheets should
//! declare `media="print"` (flipped to `all` via onload) or a specific
//! media query so the browser can defer them.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "perf-no-render-blocking-css",
    description: "`<link rel=\"stylesheet\">` without a `media` attribute blocks first paint.",
    remediation: "Add a `media` attribute (e.g. `media=\"print\" onLoad=\"this.media='all'\"`) or inline critical CSS.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["web-performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
