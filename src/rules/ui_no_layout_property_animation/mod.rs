//! ui-no-layout-property-animation — flag inline `transition` /
//! `transitionProperty` styles that animate layout-triggering properties
//! (width, height, top, left, right, bottom, margin, padding, border).
//! Layout-triggering animations cause layout recalculation every frame.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-layout-property-animation",
    description: "Inline `transition` references a layout-triggering property — animating layout \
                  causes per-frame layout recalculation.",
    remediation: "Animate `transform`, `opacity`, `color`, `background`, or `filter` instead — \
                  they don't trigger layout.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
