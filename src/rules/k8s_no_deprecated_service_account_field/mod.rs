//! k8s-no-deprecated-service-account-field — pod spec must not use the deprecated `serviceAccount` key.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-no-deprecated-service-account-field",
    description: "Pod spec uses the deprecated `serviceAccount` field; use `serviceAccountName`.",
    remediation: "Rename `serviceAccount` to `serviceAccountName` in the pod spec.",
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
