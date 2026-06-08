//! k8s-require-pod-disruption-budget — Deployments/StatefulSets need a PDB.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-require-pod-disruption-budget",
    description: "Each Deployment/StatefulSet should have an accompanying PodDisruptionBudget.",
    remediation: "Author a PodDisruptionBudget (minAvailable or maxUnavailable) targeting this workload.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["kubernetes"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Yaml, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
