//! k8s-require-ingress-tls — Ingress must define spec.tls.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-require-ingress-tls",
    description: "Ingress resources must declare spec.tls to terminate TLS.",
    remediation: "Add a `spec.tls` section with `hosts` and `secretName` referencing a TLS certificate.",
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
