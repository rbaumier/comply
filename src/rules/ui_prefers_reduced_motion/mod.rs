//! ui-prefers-reduced-motion — CSS that declares animations or transitions
//! must include a `@media (prefers-reduced-motion: reduce)` branch.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-prefers-reduced-motion",
    description: "CSS declaring animation or transition must provide a `@media (prefers-reduced-motion: reduce)` branch.",
    remediation: "Wrap motion-sensitive declarations in `@media (prefers-reduced-motion: reduce) { ... }` that disables them.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui", "a11y"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Css, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
