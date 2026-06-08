//! k8s-no-secret-in-env-literal — Don't hardcode secrets in container env literals.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-no-secret-in-env-literal",
    description: "Sensitive env vars (PASSWORD, TOKEN, SECRET, API_KEY) must not use a literal `value:`.",
    remediation: "Reference a Secret via `valueFrom.secretKeyRef`, or mount the secret as a file.",
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
