//! k8s-rolling-update-zero-unavailable — strategy.rollingUpdate.maxUnavailable must be 0.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-rolling-update-zero-unavailable",
    description: "Deployment strategy.rollingUpdate.maxUnavailable must be 0 to avoid downtime.",
    remediation: "Set `spec.strategy.rollingUpdate.maxUnavailable: 0` and use `maxSurge` to add capacity during rollout.",
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
