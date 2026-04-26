//! k8s-no-allow-privileged-scc — SecurityContextConstraints must not allow privileged containers.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-no-allow-privileged-scc",
    description: "OpenShift SecurityContextConstraints must not set allowPrivilegedContainer: true.",
    remediation: "Set `allowPrivilegedContainer: false` on the SecurityContextConstraints, or remove the field.",
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
