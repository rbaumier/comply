//! k8s-rbac-no-wildcard-verbs — Role/ClusterRole must not use verbs: ["*"].

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-rbac-no-wildcard-verbs",
    description: "RBAC rules must not grant verbs: [\"*\"]; enumerate the verbs needed.",
    remediation: "Replace `verbs: [\"*\"]` with the specific verbs required (get, list, watch, create, update, patch, delete).",
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
