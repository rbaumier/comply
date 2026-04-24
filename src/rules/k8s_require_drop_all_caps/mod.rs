//! k8s-require-drop-all-caps — securityContext.capabilities.drop must include ALL.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-require-drop-all-caps",
    description: "Containers must drop ALL Linux capabilities (securityContext.capabilities.drop includes ALL).",
    remediation: "Add `securityContext: { capabilities: { drop: [\"ALL\"] } }` to each container.",
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
