//! react-jsx-key — missing `key` prop in iterators / collection literals.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-key",
    description: "Missing `key` prop inside iterator — React needs stable keys to reconcile lists.",
    remediation: "Add a unique, stable `key` prop to each JSX element returned \
                  from `.map()`, `.flatMap()`, `.from()`, or an array literal.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/jsx-key.md"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    let backends = crate::register_ts_family!(META, react).backends;
    RuleDef { meta: META, backends }
}
