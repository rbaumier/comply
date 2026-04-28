//! react-prefer-use-reducer — components with many `useState` calls are usually
//! managing related state that belongs in a single reducer.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-prefer-use-reducer",
    description: "Component has 4 or more `useState` calls — likely related state.",
    remediation: "Combine related state into a single `useReducer` to keep \
                  transitions consistent and reduce re-renders.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
