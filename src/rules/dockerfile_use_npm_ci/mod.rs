//! dockerfile-use-npm-ci — `npm install` mutates the lockfile and slows image
//! builds; CI builds must use `npm ci`.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-use-npm-ci",
    description: "`npm install` is non-deterministic in images; use `npm ci`.",
    remediation: "Replace `RUN npm install` with `RUN npm ci`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["docker"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(
            Language::Dockerfile,
            Backend::TreeSitter(Box::new(typescript::Check)),
        )],
    }
}
