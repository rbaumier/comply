//! ci-setup-node-cache-enabled

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ci-setup-node-cache-enabled",
    description: "actions/setup-node without `cache:` re-downloads the npm registry on \
                  every run, wasting minutes and bandwidth.",
    remediation: "Add `cache: 'npm'` (or 'pnpm'/'yarn') inside the step's `with:` block.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ci-cd"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Yaml, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
