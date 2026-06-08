//! dockerfile-no-shell-utils-in-run — interactive or system-management tools
//! have no place in a non-interactive container build.

mod check;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-no-shell-utils-in-run",
    description: "Interactive/system tools (ssh, vim, top, kill, ...) do not belong in a RUN.",
    remediation: "Remove these commands from the Dockerfile; configure tooling outside of image build.",
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
            Backend::TreeSitter(Box::new(check::Check)),
        )],
    }
}
