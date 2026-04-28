//! react-no-adjacent-inline-elements — adjacent inline elements without spacing.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-adjacent-inline-elements",
    description: "Adjacent inline elements without whitespace between them.",
    remediation: "Add a space, `{' '}`, or a wrapper between adjacent inline \
                  elements to ensure they render with visible separation.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/no-adjacent-inline-elements.md"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    let backends = crate::register_ts_family!(META, react).backends;
    RuleDef { meta: META, backends }
}
