//! ci-checkout-action-pinned

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ci-checkout-action-pinned",
    description: "actions/checkout must be pinned to v4 or higher — older versions and \
                  floating refs (@main, @master) expose workflows to breaking changes and \
                  supply-chain drift.",
    remediation: "Use `uses: actions/checkout@v4` (or a commit SHA). Never pin to @main, \
                  @master, or @v3 or lower.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ci-cd"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Yaml, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
