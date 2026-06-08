//! react-use-state-lazy-init — wrap expensive useState inits in a function.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-use-state-lazy-init",
    description: "`useState(expensive())` runs on every render.",
    remediation: "Wrap the initializer in a lazy function: \
                  `useState(() => expensive())`. Passing a function means \
                  React only calls it once on mount. Bare expressions run \
                  every render and crash in SSR for browser APIs.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "react"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check)))],
    }
}
