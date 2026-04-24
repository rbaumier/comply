//! react-no-render-return-value — flag capturing the return value of
//! `ReactDOM.render()`.
//!
//! Why: since React 16 the value returned by `ReactDOM.render()` is
//! unreliable and discouraged. Code that relies on it (assigning the
//! root component instance) breaks under concurrent rendering and
//! silently returns `null` in many cases.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-render-return-value",
    description: "Do not use the return value of `ReactDOM.render()`.",
    remediation: "Call `ReactDOM.render()` as a statement; attach refs via `ref` callbacks instead.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/no-render-return-value.md",
    ),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
