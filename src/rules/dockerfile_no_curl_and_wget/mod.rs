//! dockerfile-no-curl-and-wget — flag images that mix `curl` and `wget`,
//! since carrying both bloats the image. Hadolint DL4001.

mod check;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-no-curl-and-wget",
    description: "Dockerfile uses both curl and wget; pick one to reduce image size.",
    remediation: "Use either `curl` or `wget` consistently across the Dockerfile.",
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
