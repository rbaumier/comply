//! k8s-require-standard-labels — resources must include Kubernetes standard labels.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-require-standard-labels",
    description: "Resources must include app.kubernetes.io/name and app.kubernetes.io/instance labels.",
    remediation: "Add `app.kubernetes.io/name` and `app.kubernetes.io/instance` under metadata.labels.",
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
