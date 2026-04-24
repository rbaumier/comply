//! compose-cap-drop-all — every service must drop all Linux capabilities
//! (`cap_drop: [ALL]`) and re-add only what's required.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "compose-cap-drop-all",
    description: "Each service must declare `cap_drop: [ALL]` (and re-add specific caps via `cap_add:`).",
    remediation: "Add `cap_drop: [ALL]` under every service block.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["docker", "docker-compose"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Yaml, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
