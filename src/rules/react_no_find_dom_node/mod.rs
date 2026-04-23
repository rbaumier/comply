//! react-no-find-dom-node — disallow `ReactDOM.findDOMNode()` and bare `findDOMNode()`.
//!
//! `findDOMNode` is deprecated in React 19 (removed from `react-dom`). It
//! breaks encapsulation, forces synchronous DOM access, and is incompatible
//! with future React rendering modes. Refs are the supported alternative.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-find-dom-node",
    description: "`findDOMNode` is deprecated in React 19 — use refs instead.",
    remediation: "Use refs instead of findDOMNode.",
    severity: Severity::Warning,
    doc_url: Some("https://react.dev/reference/react-dom/findDOMNode"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
