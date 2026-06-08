//! k8s-dangling-hpa — HPA scaleTargetRef must point to an existing workload.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-dangling-hpa",
    description: "HorizontalPodAutoscaler.spec.scaleTargetRef must reference a resource that exists in the project.",
    remediation: "Create the referenced workload (Deployment, StatefulSet, ...) or fix the scaleTargetRef kind/name/namespace.",
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
