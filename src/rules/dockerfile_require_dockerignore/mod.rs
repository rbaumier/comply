//! dockerfile-require-dockerignore — `COPY .` without an explicit
//! `.dockerignore` acknowledgement risks shipping local junk into the image.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-require-dockerignore",
    description: "Dockerfile uses broad `COPY .`; ensure a `.dockerignore` file excludes build artefacts and secrets.",
    remediation: "Add a `.dockerignore` (mention it in a comment above the COPY) so `node_modules`, `.env`, `.git`, etc. don't leak into the image.",
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
