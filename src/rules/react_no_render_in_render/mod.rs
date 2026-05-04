//! react-no-render-in-render — inline `renderXxx()` calls in JSX should be
//! extracted to standalone components for proper reconciliation.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-render-in-render",
    description: "Inline `renderXxx()` call in JSX — extract to a component for proper reconciliation.",
    remediation: "Replace `{renderHeader()}` with a `<Header />` component. \
                  Inline render functions bypass React's reconciliation, causing \
                  unnecessary DOM destruction and state loss on every render.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
