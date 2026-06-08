//! dockerfile-no-latest-tag — FROM image must pin a version tag; `:latest` or
//! no tag at all allows silent base-image drift.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-no-latest-tag",
    description: "FROM image must pin a version tag; `:latest` and untagged images drift silently.",
    remediation: "Replace `:latest` (or missing tag) with a pinned version such as `node:22.12-alpine3.20`.",
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
            Backend::TreeSitter(Box::new(typescript::Check)),
        )],
    }
}
