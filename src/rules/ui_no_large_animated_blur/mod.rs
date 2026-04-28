//! ui-no-large-animated-blur — flag inline `filter: blur(Npx)` styles where
//! the blur radius exceeds 20px. Large blur radii are expensive (cost grows
//! with radius and layer size) and can exhaust GPU memory on mobile.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-large-animated-blur",
    description: "Inline `filter: blur(Npx)` with N > 20 — expensive, escalates with radius and \
                  layer size, can exhaust GPU memory on mobile.",
    remediation: "Reduce the blur radius below 20px, or composite the blur statically into a \
                  background image.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
