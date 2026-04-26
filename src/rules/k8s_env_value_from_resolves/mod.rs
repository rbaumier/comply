//! k8s-env-value-from-resolves — container env `valueFrom` references must
//! resolve to an existing Secret/ConfigMap in the same namespace.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-env-value-from-resolves",
    description: "Container env `valueFrom.secretKeyRef`/`configMapKeyRef` must reference an existing resource.",
    remediation: "Create the missing Secret/ConfigMap in the namespace, or fix the reference name.",
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
