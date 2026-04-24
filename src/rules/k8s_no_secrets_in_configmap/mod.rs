//! k8s-no-secrets-in-configmap — ConfigMap must not contain secret-shaped keys.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-no-secrets-in-configmap",
    description: "ConfigMap data must not contain secret-looking keys (PASSWORD, TOKEN, KEY, SECRET).",
    remediation: "Move the value into a Kubernetes Secret (or sealed/external secret) and mount or reference it.",
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
