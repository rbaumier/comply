//! k8s-min-replicas-two — Deployments must declare replicas >= 2.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-min-replicas-two",
    description: "Deployments must have replicas >= 2 (or HPA minReplicas >= 2) for availability.",
    remediation: "Set `spec.replicas: 2` or higher, or use a HorizontalPodAutoscaler with minReplicas >= 2.",
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
