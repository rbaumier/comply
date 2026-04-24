//! compose-no-privileged — `privileged: true` grants kernel-level access and
//! must not appear in shipped compose files.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "compose-no-privileged",
    description: "Services must not set `privileged: true`.",
    remediation: "Remove `privileged: true`; grant only the specific capabilities required via `cap_add:`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["docker", "docker-compose"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Yaml, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
