//! ui-hover-gated-media-query — `:hover { transform: ... }` should live
//! inside `@media (hover: hover) and (pointer: fine)` so touch devices
//! don't stick in the hovered state.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-hover-gated-media-query",
    description: "`:hover` rules that move/transform should be gated by `@media (hover: hover) and (pointer: fine)`.",
    remediation: "Wrap the `:hover { transform/scale … }` declaration in `@media (hover: hover) and (pointer: fine) { ... }`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Css, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
