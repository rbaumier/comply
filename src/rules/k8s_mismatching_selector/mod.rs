//! k8s-mismatching-selector — selector.matchLabels must match template labels.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-mismatching-selector",
    description: "Deployment selector.matchLabels must match spec.template.metadata.labels.",
    remediation: "Ensure spec.selector.matchLabels is a subset of spec.template.metadata.labels.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["kubernetes"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Yaml, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
