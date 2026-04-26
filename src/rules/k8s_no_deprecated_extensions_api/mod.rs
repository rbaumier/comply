//! k8s-no-deprecated-extensions-api — Reject the removed `extensions/v1beta*` apiVersion.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-no-deprecated-extensions-api",
    description: "Manifests must not use the removed `extensions/v1beta*` apiVersion.",
    remediation: "Migrate to `apps/v1` (Deployment/DaemonSet/ReplicaSet), `networking.k8s.io/v1` (Ingress/NetworkPolicy), or `policy/v1` (PodSecurityPolicy successors).",
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
