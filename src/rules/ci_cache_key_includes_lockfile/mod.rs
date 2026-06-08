//! ci-cache-key-includes-lockfile

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ci-cache-key-includes-lockfile",
    description: "An `actions/cache` key that doesn't include `hashFiles(...)` of the \
                  lockfile never invalidates — stale caches break reproducibility.",
    remediation: "Include `${{ hashFiles('**/package-lock.json') }}` (or the pnpm/yarn \
                  equivalent) in the cache `key:`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ci-cd"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Yaml, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
