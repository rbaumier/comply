//! k8s-pdb-eviction-policy — PDBs must declare an unhealthyPodEvictionPolicy.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-pdb-eviction-policy",
    description: "PodDisruptionBudget must declare `spec.unhealthyPodEvictionPolicy` to allow eviction of unhealthy pods.",
    remediation: "Add `spec.unhealthyPodEvictionPolicy: AlwaysAllow` (or `IfHealthyBudget`) to the PDB.",
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
