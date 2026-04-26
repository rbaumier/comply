//! k8s-hpa-min-three-replicas — HPA minReplicas must be >= 3.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-hpa-min-three-replicas",
    description: "HorizontalPodAutoscaler minReplicas must be at least 3 to survive node drains.",
    remediation: "Set `spec.minReplicas: 3` (or higher) on the HorizontalPodAutoscaler.",
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
