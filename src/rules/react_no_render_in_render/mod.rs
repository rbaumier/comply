//! react-no-render-in-render — inline `renderXxx()` calls in JSX should be
//! extracted to standalone components for proper reconciliation.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

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
    crate::register_ts_family!(META, react)
}
