//! react-no-typos — common React lifecycle / static property typos.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-typos",
    description: "Probable typo in React component static property or lifecycle method.",
    remediation: "Fix the typo. Common mistakes include `getDerivedStateFromProp` \
                  (should be `getDerivedStateFromProps`) and `componentWillRecieveProps` \
                  (should be `componentWillReceiveProps`).",
    severity: Severity::Error,
    doc_url: Some(
        "https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/no-typos.md",
    ),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
