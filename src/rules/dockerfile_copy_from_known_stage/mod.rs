//! dockerfile-copy-from-known-stage — `COPY --from=<name>` must reference a
//! previously-defined build stage alias.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-copy-from-known-stage",
    description: "`COPY --from=<name>` must reference a known build stage.",
    remediation: "Define the stage with `FROM ... AS <name>` or fix the typo.",
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
