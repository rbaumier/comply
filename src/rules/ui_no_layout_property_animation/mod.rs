//! ui-no-layout-property-animation — flag inline `transition` /
//! `transitionProperty` styles that animate layout-triggering properties
//! (width, height, top, left, right, bottom, margin, padding, border).
//! Layout-triggering animations cause layout recalculation every frame.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-layout-property-animation",
    description: "Inline `transition` references a layout-triggering property — animating layout \
                  causes per-frame layout recalculation.",
    remediation: "Animate `transform`, `opacity`, `color`, `background`, or `filter` instead — \
                  they don't trigger layout.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
