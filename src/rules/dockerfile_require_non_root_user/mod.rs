//! dockerfile-require-non-root-user — the final stage must drop to a
//! non-root user before CMD.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-require-non-root-user",
    description: "Production Dockerfile must declare a non-root USER.",
    remediation: "Add `USER <non-root>` (and create the user if needed) before CMD/ENTRYPOINT.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["docker"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Dockerfile, Backend::TreeSitter(Box::new(typescript::Check)))],
    }
}
