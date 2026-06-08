//! dockerfile-use-frozen-lockfile — pnpm/yarn installs in images must use
//! `--frozen-lockfile` to refuse lockfile drift.

mod check;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-use-frozen-lockfile",
    description: "pnpm/yarn install in Dockerfiles must use `--frozen-lockfile`.",
    remediation: "Pass `--frozen-lockfile` to `pnpm install` / `yarn install` so the build fails on lockfile drift.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["docker"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(
            Language::Dockerfile,
            Backend::TreeSitter(Box::new(check::Check)),
        )],
    }
}
