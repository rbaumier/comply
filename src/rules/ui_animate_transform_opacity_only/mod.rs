//! ui-animate-transform-opacity-only — keyframes should only animate
//! transform and opacity to stay on the compositor thread.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-animate-transform-opacity-only",
    description: "Animations should only target `transform` and `opacity`; other properties trigger layout/paint.",
    remediation: "Rewrite the `@keyframes` to animate `transform` / `opacity`; use layout tricks or FLIP instead of animating top/left/width/height/margin/padding.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Css, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
