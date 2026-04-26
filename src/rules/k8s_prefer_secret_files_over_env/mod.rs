//! k8s-prefer-secret-files-over-env — Prefer mounting secrets as files over env vars.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-prefer-secret-files-over-env",
    description: "Container env entries should not source values from `secretKeyRef`; mount the Secret as a file instead.",
    remediation: "Mount the Secret via `volumes`/`volumeMounts` and read the value from the filesystem; env vars leak through `kubectl describe` and child processes.",
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
