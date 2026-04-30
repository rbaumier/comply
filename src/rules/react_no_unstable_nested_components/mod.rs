//! react-no-unstable-nested-components — component defined inside render.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-unstable-nested-components",
    description: "Component defined inside another component causes unmount/remount every render.",
    remediation: "Move the inner component outside the parent component. Defining a \
                  component inside render means React sees a brand-new type on every \
                  render, destroying the entire subtree's DOM nodes and state.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
