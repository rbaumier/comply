//! react-jsx-no-jsx-as-prop — disallow JSX elements/fragments passed as prop values.
//!
//! Passing a JSX element/fragment inline as a prop (`<Comp icon={<Icon />} />`)
//! creates a fresh element object on every render, breaking referential equality
//! and forcing memoized children to re-render. Extract to a variable or `useMemo`.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-no-jsx-as-prop",
    description: "JSX elements/fragments passed directly as prop values cause unnecessary re-renders.",
    remediation: "Extract JSX to a variable or use useMemo so the prop reference is stable across renders.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
