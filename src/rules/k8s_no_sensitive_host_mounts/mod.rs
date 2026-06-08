//! k8s-no-sensitive-host-mounts — pod must not mount sensitive host paths.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-no-sensitive-host-mounts",
    description: "Pod mounts a sensitive host path (e.g. /, /etc, /proc); container can compromise the node.",
    remediation: "Avoid mounting system paths from the host; use a dedicated data volume instead.",
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
