//! k8s-require-run-as-non-root — securityContext.runAsNonRoot must be true.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-require-run-as-non-root",
    description: "Containers must set securityContext.runAsNonRoot: true.",
    remediation: "Add `securityContext: { runAsNonRoot: true }` at the pod or container level.",
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
