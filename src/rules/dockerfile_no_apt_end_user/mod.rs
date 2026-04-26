//! dockerfile-no-apt-end-user — `apt` is the human-facing front-end and its
//! output is not stable; use `apt-get` in scripts.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-no-apt-end-user",
    description: "Use `apt-get` instead of the end-user `apt` command.",
    remediation: "Replace `apt` with `apt-get` (or `apt-cache`) inside Dockerfile RUNs.",
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
