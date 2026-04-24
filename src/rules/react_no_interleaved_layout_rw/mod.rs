//! react-no-interleaved-layout-rw — layout-read/style-write interleaving (layout thrash).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-interleaved-layout-rw",
    description: "Reads of layout properties (`offsetWidth`, `getBoundingClientRect`, …) \
                  interleaved with `.style.*` writes in the same function force sync \
                  layout on every write.",
    remediation: "Batch reads first, writes second — or schedule writes inside \
                  `requestAnimationFrame` after all reads complete.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react", "web-performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
