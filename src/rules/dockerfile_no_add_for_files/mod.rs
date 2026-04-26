//! dockerfile-no-add-for-files — ADD silently extracts archives and fetches
//! URLs; for plain files COPY is the predictable choice.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-no-add-for-files",
    description: "Use COPY instead of ADD for plain files and folders.",
    remediation: "Replace ADD with COPY when the source is a regular file or directory.",
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
