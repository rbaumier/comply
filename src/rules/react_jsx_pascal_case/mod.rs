//! react-jsx-pascal-case — enforce PascalCase for user-defined JSX components.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-pascal-case",
    description: "User-defined JSX components must use PascalCase.",
    remediation: "Rename the component to PascalCase (e.g., `MyComponent` instead \
                  of `my_component` or `myComponent`).",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/jsx-pascal-case.md"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    let backends = crate::register_ts_family!(META, react).backends;
    RuleDef { meta: META, backends }
}
