//! react-jsx-no-useless-fragment — unnecessary `<Fragment>` / `<>` wrappers.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-no-useless-fragment",
    description: "Unnecessary `<Fragment>` that wraps a single child or nothing.",
    remediation: "Remove the fragment wrapper when it contains only one child or \
                  is empty. Fragments are only needed to group multiple siblings.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/jsx-no-useless-fragment.md"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
