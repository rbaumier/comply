//! k8s-no-exposed-services — Reject Service types that expose pods outside the cluster.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-no-exposed-services",
    description: "Service.spec.type must not be NodePort or LoadBalancer; expose via Ingress / Gateway instead.",
    remediation: "Use `type: ClusterIP` and route external traffic through an Ingress, Gateway, or service-mesh gateway.",
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
