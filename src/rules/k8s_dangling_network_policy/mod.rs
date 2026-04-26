//! k8s-dangling-network-policy — NetworkPolicy.spec.podSelector must match at least one pod.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-dangling-network-policy",
    description: "NetworkPolicy.spec.podSelector.matchLabels must match at least one workload's pod template labels.",
    remediation: "Align the NetworkPolicy podSelector with a workload's pod template labels, or remove the unused NetworkPolicy.",
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
