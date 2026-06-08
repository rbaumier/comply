//! k8s-rbac-no-secret-access — Role/ClusterRole must not grant read access on secrets.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-rbac-no-secret-access",
    description: "RBAC rules must not grant get/list/watch on `secrets`.",
    remediation: "Avoid granting read access on secrets; use a dedicated service account scoped to specific secret names if absolutely required.",
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
