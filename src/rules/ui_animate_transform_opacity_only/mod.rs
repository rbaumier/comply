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
    remediation: "Animate `transform` instead. Use `translate()` in place of top/left/right/bottom and of margin/padding offsets. Use `scale()` with a matching `transform-origin` in place of width/height. A collapse that must reflow its siblings has no `transform`/`opacity` form. Animate `grid-template-rows` from `0fr` to `1fr`, or `height` under `interpolate-size: allow-keywords`.",
    severity: Severity::Error,
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
