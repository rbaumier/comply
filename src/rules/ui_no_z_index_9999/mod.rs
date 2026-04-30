//! ui-no-z-index-9999 — z-index values above 100 are almost always a sign of
//! a z-index arms race.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-z-index-9999",
    description: "`z-index` value above 100 — use a structured layering system instead.",
    remediation: "Define z-index layers as named constants (e.g. `Z_MODAL = 50`, \
                  `Z_TOOLTIP = 60`) and keep values under 100.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
