//! dockerfile-require-healthcheck — production images must ship a
//! HEALTHCHECK so orchestrators can detect stuck containers.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-require-healthcheck",
    description: "Production Dockerfile must declare a HEALTHCHECK.",
    remediation: "Add `HEALTHCHECK --interval=30s CMD curl -f http://localhost:PORT/health || exit 1`.",
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
