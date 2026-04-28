//! react-no-derived-use-state — `useState(propName)` initialized from a prop
//! should be derived during render instead.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-derived-use-state",
    description: "`useState` initialized from a prop — derive the value during render instead.",
    remediation: "Remove the `useState` and compute the value inline during render, \
                  or use the `key` prop on the component to reset state when the prop \
                  changes. Copying props into state causes stale values.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
