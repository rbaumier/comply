//! dockerfile-apt-clean-lists — every `apt-get install` should clean the apt
//! cache in the same layer to keep the resulting image small.

mod check;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-apt-clean-lists",
    description: "`apt-get install` must clean `/var/lib/apt/lists/*` in the same RUN layer.",
    remediation: "Append `&& rm -rf /var/lib/apt/lists/*` to the RUN that runs apt-get install.",
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
