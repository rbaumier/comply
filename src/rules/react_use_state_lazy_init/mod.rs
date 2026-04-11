//! react-use-state-lazy-init — wrap expensive useState inits in a function.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

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
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Tsx, Backend::TreeSitter(Box::new(typescript::Check)))],
    }
}
