//! k8s-deployment-anti-affinity — Multi-replica Deployments must declare podAntiAffinity.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-deployment-anti-affinity",
    description: "Deployments with replicas > 1 must declare `spec.template.spec.affinity.podAntiAffinity`.",
    remediation: "Add a podAntiAffinity (preferred or required) so replicas are spread across nodes/zones.",
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
