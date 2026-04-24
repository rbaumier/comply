//! perf-img-fetchpriority-high — flag LCP/hero images without fetchpriority="high",
//! and reject conflicting `fetchpriority="high"` + `loading="lazy"` combos.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "perf-img-fetchpriority-high",
    description: "Hero/LCP images should declare `fetchpriority=\"high\"` and must not be lazy-loaded.",
    remediation: "Add `fetchpriority=\"high\"` to the LCP image, and remove `loading=\"lazy\"` on it.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["web-performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
