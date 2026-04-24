//! compose-no-inline-secrets — `environment:` entries with secret-like keys
//! must not carry literal values; use `env_file:` or `secrets:`.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "compose-no-inline-secrets",
    description: "docker-compose `environment:` must not embed secret literals.",
    remediation: "Move secret values to `env_file:` or `secrets:` and reference variables by name.",
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
