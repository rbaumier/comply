//! react-jsx-props-no-spread-multi — same identifier spread multiple times.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-props-no-spread-multi",
    description: "Same object spread multiple times on a JSX element.",
    remediation: "Remove the duplicate spread. Spreading the same identifier \
                  twice is likely a copy-paste mistake.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/jsx-props-no-spread-multi.md"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    let backends = crate::register_ts_family!(META, react).backends;
    RuleDef { meta: META, backends }
}
