//! ci-docker-gha-cache

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ci-docker-gha-cache",
    description: "`docker/build-push-action` without `cache-from`/`cache-to: type=gha` \
                  rebuilds every layer from scratch on each run, burning CI minutes.",
    remediation: "Add `cache-from: type=gha` and `cache-to: type=gha,mode=max` to the \
                  step's `with:` block.",
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
