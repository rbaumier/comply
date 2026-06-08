//! k8s-no-plaintext-secret-in-git — forbid populated Secret manifests in source.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-no-plaintext-secret-in-git",
    description: "kind: Secret manifests must not have populated data/stringData fields in source control.",
    remediation: "Use SealedSecrets, External Secrets Operator, SOPS, or inject at deploy time.",
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
