//! k8s-dangling-network-policy-peer — NetworkPolicy ingress/egress peer podSelectors must match pods.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-dangling-network-policy-peer",
    description: "NetworkPolicy ingress/egress peer podSelectors must match at least one workload's pod template labels.",
    remediation: "Align the peer podSelector with a workload's pod template labels, or remove the dangling rule.",
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
