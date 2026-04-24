//! compose-no-latest-tag — `image:` values in docker-compose must pin a
//! precise tag.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "compose-no-latest-tag",
    description: "docker-compose `image:` values must pin a tag (no `:latest`, no missing tag).",
    remediation: "Replace `:latest` or untagged images with a precise pin like `postgres:16.6-alpine3.20`.",
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
