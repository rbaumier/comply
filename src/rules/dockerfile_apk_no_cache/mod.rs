//! dockerfile-apk-no-cache — `apk add` should use `--no-cache` to avoid
//! leaving the apk index in the resulting layer.

mod check;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-apk-no-cache",
    description: "Use `apk add --no-cache` instead of `apk add`.",
    remediation: "Add `--no-cache` to `apk add` to avoid leaving the apk index in the layer.",
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
