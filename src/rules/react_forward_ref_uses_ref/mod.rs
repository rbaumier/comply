//! react-forward-ref-uses-ref — `forwardRef` without using the ref param.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-forward-ref-uses-ref",
    description: "`forwardRef` component does not use the `ref` parameter.",
    remediation: "Either use the `ref` parameter in the component body or remove \
                  the `forwardRef` wrapper — it serves no purpose without a ref.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/forward-ref-uses-ref.md"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
