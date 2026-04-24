//! k8s-require-explicit-namespace — manifests must set metadata.namespace.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-require-explicit-namespace",
    description: "Namespaced resources must declare metadata.namespace explicitly.",
    remediation: "Add `metadata.namespace: <name>` rather than relying on the implicit `default` namespace.",
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
