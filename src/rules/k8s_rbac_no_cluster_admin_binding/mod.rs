//! k8s-rbac-no-cluster-admin-binding — bindings must not target `cluster-admin`.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-rbac-no-cluster-admin-binding",
    description: "RoleBinding/ClusterRoleBinding must not bind to the `cluster-admin` role.",
    remediation: "Bind to a role with the minimum permissions required instead of `cluster-admin`.",
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
