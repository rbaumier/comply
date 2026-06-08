//! k8s-non-existent-service-account — pod spec.serviceAccountName must reference an existing ServiceAccount.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-non-existent-service-account",
    description: "Workload spec.serviceAccountName must reference a ServiceAccount that exists in the project.",
    remediation: "Create the referenced ServiceAccount in the matching namespace, or fix the serviceAccountName value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["kubernetes"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Yaml, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
