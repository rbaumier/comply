//! perf-prefers-reduced-motion — any CSS file that declares animations or
//! `@keyframes` must also provide a `@media (prefers-reduced-motion: reduce)`
//! branch that tames them.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "perf-prefers-reduced-motion",
    description: "CSS with animations or `@keyframes` must guard them with a `prefers-reduced-motion: reduce` media query.",
    remediation: "Add `@media (prefers-reduced-motion: reduce) { ... }` that disables or shortens animations.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["web-performance", "a11y"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Css, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
