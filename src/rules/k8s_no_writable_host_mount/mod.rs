//! k8s-no-writable-host-mount — pod must not mount hostPath volumes writable.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-no-writable-host-mount",
    description: "Pod uses a hostPath volume; mount it as readOnly or remove the host mount entirely.",
    remediation: "Avoid hostPath volumes; if required, mark the corresponding volumeMount as `readOnly: true`.",
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
