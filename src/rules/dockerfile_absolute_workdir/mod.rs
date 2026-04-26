//! dockerfile-absolute-workdir — WORKDIR must use an absolute path so the
//! working directory is deterministic regardless of the previous WORKDIR.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-absolute-workdir",
    description: "WORKDIR must use an absolute path.",
    remediation: "Replace the relative WORKDIR path with an absolute path starting with `/`.",
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
