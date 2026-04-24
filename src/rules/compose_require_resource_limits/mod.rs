//! compose-require-resource-limits — every service must set
//! `deploy.resources.limits.memory` to prevent runaway containers.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "compose-require-resource-limits",
    description: "Each service must declare `deploy.resources.limits.memory`.",
    remediation: "Add a `deploy.resources.limits.memory` entry (e.g. `memory: 512M`) to every service.",
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
