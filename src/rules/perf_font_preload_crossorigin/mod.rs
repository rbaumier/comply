//! perf-font-preload-crossorigin — `<link rel="preload" as="font">` must
//! declare `crossorigin` (fonts are always fetched in CORS mode) and
//! `type="font/woff2"` so the preload can match the CSSOM request.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "perf-font-preload-crossorigin",
    description: "`<link rel=\"preload\" as=\"font\">` must include `crossorigin` and `type=\"font/woff2\"`.",
    remediation: "Add `crossorigin` and `type=\"font/woff2\"` to the font preload link.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["web-performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
