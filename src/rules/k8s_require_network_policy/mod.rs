//! k8s-require-network-policy — remind to author a NetworkPolicy for workloads.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-require-network-policy",
    description: "Each Deployment should have an accompanying NetworkPolicy; namespaces should not rely on default-allow.",
    remediation: "Author a NetworkPolicy that whitelists ingress/egress for this workload.",
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
