//! ui-concentric-border-radius — nested rounded blocks should follow the
//! concentric rule: child radius = parent radius − parent padding.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-concentric-border-radius",
    description: "A child with `border-radius` inside a rounded + padded parent should use `calc(parent-radius - parent-padding)`.",
    remediation: "Express the child radius via `calc(var(--radius) - var(--padding))` to stay concentric.",
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
