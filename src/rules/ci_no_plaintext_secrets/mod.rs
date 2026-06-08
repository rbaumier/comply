//! ci-no-plaintext-secrets

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ci-no-plaintext-secrets",
    description: "Workflow `env:`/`with:` values whose key mentions password, token, \
                  secret, or api_key must not be literal strings — they leak into logs, \
                  forks and git history.",
    remediation: "Reference the value via `${{ secrets.<NAME> }}` from repository or \
                  environment secrets.",
    severity: Severity::Error,
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
