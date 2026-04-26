//! k8s-no-docker-sock-mount — pod must not mount the docker socket from the host.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-no-docker-sock-mount",
    description: "Pod mounts the host docker socket; this grants full root on the host.",
    remediation: "Remove the docker.sock hostPath volume; use a sidecar with rootless tooling instead.",
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
