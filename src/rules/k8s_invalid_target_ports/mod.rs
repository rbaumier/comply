//! k8s-invalid-target-ports — port names must follow IANA conventions.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-invalid-target-ports",
    description: "Port names must conform to IANA naming: lowercase, alphanumeric, hyphens, 1-15 chars, start/end with alphanumeric.",
    remediation: "Rename the port to match IANA conventions (e.g. `http`, `grpc`, `metrics`).",
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
