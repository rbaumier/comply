//! react-style-prop-object — style prop must be an object, not a string.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-style-prop-object",
    description: "The `style` prop expects an object, not a CSS string.",
    remediation: "Use `style={{ color: 'red' }}` instead of `style=\"color: red\"`. \
                  React's `style` prop takes a JavaScript object with camelCase \
                  property names, not a CSS string.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
