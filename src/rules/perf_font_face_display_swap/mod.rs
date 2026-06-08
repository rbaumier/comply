//! perf-font-face-display-swap — every `@font-face` block should include
//! `font-display: swap` (or `optional`/`fallback`) to avoid invisible text
//! during webfont load.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "perf-font-face-display-swap",
    description: "Every `@font-face` block must declare `font-display: swap`.",
    remediation: "Add `font-display: swap;` inside the `@font-face` rule.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["web-performance"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Css, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
