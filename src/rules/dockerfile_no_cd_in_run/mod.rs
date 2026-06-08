//! dockerfile-no-cd-in-run — `cd` inside RUN does not affect later layers;
//! WORKDIR is the correct directive for setting the build/run directory.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-no-cd-in-run",
    description: "Use WORKDIR instead of `cd` inside a RUN instruction.",
    remediation: "Replace `RUN cd ...` patterns with a WORKDIR directive.",
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
