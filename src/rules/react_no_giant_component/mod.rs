//! react-no-giant-component — component body exceeding 300 lines suggests
//! the component should be broken into smaller focused components.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-giant-component",
    description: "Component body exceeds 300 lines — break into smaller focused components.",
    remediation: "Extract distinct sections (header, body, sidebar, etc.) \
                  into their own components. Large components are harder to \
                  test, review, and reason about.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
