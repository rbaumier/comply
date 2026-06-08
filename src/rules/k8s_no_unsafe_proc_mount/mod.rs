//! k8s-no-unsafe-proc-mount — containers must not use procMount: Unmasked.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-no-unsafe-proc-mount",
    description: "Container sets `securityContext.procMount: Unmasked`; this exposes /proc paths normally hidden by the runtime.",
    remediation: "Remove `procMount: Unmasked` (or set to `Default`).",
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
