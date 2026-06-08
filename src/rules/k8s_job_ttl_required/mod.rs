//! k8s-job-ttl-required — Jobs must declare `ttlSecondsAfterFinished`.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-job-ttl-required",
    description: "Job manifests must set `spec.ttlSecondsAfterFinished` so completed Jobs are garbage-collected.",
    remediation: "Add `spec.ttlSecondsAfterFinished: <seconds>` to the Job (e.g. 3600 for one hour).",
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
