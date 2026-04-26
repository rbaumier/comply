//! k8s-restart-policy-required — Standalone Pods must declare `restartPolicy`.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-restart-policy-required",
    description: "Standalone Pod manifests must explicitly set `spec.restartPolicy`.",
    remediation: "Add `spec.restartPolicy: Always` (long-running) or `OnFailure`/`Never` for batch workloads.",
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
