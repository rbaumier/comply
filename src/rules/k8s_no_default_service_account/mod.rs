//! k8s-no-default-service-account — pods must set serviceAccountName.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-no-default-service-account",
    description: "Pods must set serviceAccountName; the `default` account has no safe RBAC scope.",
    remediation: "Create a dedicated ServiceAccount and reference it via `spec.serviceAccountName`.",
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
