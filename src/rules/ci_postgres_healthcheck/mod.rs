//! ci-postgres-healthcheck

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ci-postgres-healthcheck",
    description: "A postgres service container with no `--health-cmd pg_isready` option \
                  lets downstream steps race the database startup and fail flakily.",
    remediation: "Add `options: --health-cmd pg_isready --health-interval 10s \
                  --health-timeout 5s --health-retries 5` (or equivalent) to the service.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ci-cd"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Yaml, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
