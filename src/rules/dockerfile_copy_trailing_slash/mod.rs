//! dockerfile-copy-trailing-slash — when COPY has multiple sources, the
//! destination must end with `/` so Docker treats it as a directory.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-copy-trailing-slash",
    description: "COPY destination must end with `/` when multiple sources are given.",
    remediation: "Append `/` to the COPY destination when copying multiple sources.",
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
