//! ci-no-hardcoded-db-password

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ci-no-hardcoded-db-password",
    description: "POSTGRES_PASSWORD hard-coded in a workflow leaks into logs, forks, \
                  and git history. Even a throwaway CI password is a credential.",
    remediation: "Reference a repository secret instead: \
                  `POSTGRES_PASSWORD: ${{ secrets.POSTGRES_PASSWORD }}`.",
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
