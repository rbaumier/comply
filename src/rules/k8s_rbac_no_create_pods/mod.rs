//! k8s-rbac-no-create-pods — Role/ClusterRole must not allow creating pods.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-rbac-no-create-pods",
    description: "RBAC rules must not grant `create` on `pods`; this enables privilege escalation.",
    remediation: "Remove `create` from verbs for `pods` resources, or scope to a controller resource.",
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
