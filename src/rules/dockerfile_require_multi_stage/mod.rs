//! dockerfile-require-multi-stage — single-stage images ship build tooling
//! in the runtime layer; use `FROM ... AS build` / `FROM ... AS runtime`.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-require-multi-stage",
    description: "Dockerfile must use multi-stage builds (`FROM ... AS <name>`).",
    remediation: "Split into `FROM ... AS build` and a runtime stage that `COPY --from=build` the artefacts.",
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
