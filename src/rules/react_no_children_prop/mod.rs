//! react-no-children-prop — passing children as a JSX prop.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-children-prop",
    description: "Passing `children` as a prop instead of nesting content.",
    remediation: "Place children between the opening and closing tags instead of \
                  passing them as a `children` prop. This is more readable and \
                  idiomatic.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/no-children-prop.md"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
