//! react-jsx-no-bind — `.bind()` or arrow functions in JSX props.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-no-bind",
    description: "`.bind()` or arrow function in JSX prop creates a new function on every render.",
    remediation: "Extract the handler to a stable reference (e.g., `useCallback`, \
                  a class method, or a module-level function) to avoid unnecessary \
                  re-renders of child components.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/jsx-no-bind.md"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
