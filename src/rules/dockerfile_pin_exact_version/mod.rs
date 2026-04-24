//! dockerfile-pin-exact-version — FROM tags must pin a precise version, not
//! just a major.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-pin-exact-version",
    description: "Base image tag must pin a full version (e.g. `node:22.12-alpine3.20`), not just a major.",
    remediation: "Replace bare-major tags like `:22` with a precise pin such as `22.12-alpine3.20`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["docker"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Dockerfile, Backend::Text(Box::new(text::Check)))],
    }
}
