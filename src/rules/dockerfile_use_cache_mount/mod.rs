//! dockerfile-use-cache-mount — package manager RUN steps must use
//! `--mount=type=cache` so rebuilds don't re-download.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-use-cache-mount",
    description: "Package manager RUN steps must use `--mount=type=cache`.",
    remediation: "Prefix the RUN with `--mount=type=cache,target=<cache-dir>` for the tool in use (npm, pnpm, pip, apt).",
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
