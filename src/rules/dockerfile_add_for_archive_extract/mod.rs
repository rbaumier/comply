//! dockerfile-add-for-archive-extract — ADD with a URL leaves the downloaded
//! file in the layer; use RUN curl/wget + tar instead.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-add-for-archive-extract",
    description: "ADD should not be used to fetch URLs; use RUN curl/wget + tar.",
    remediation: "Replace `ADD <url>` with a `RUN curl ... && tar ...` pipeline that cleans up afterwards.",
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
