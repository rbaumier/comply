//! ci-use-npm-ci

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ci-use-npm-ci",
    description: "`npm install` mutates the lockfile and installs without strict \
                  reproducibility. CI must use `npm ci` to install exactly what the \
                  lockfile describes.",
    remediation: "Replace `run: npm install` with `run: npm ci` in the workflow.",
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
