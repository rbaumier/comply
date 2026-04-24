//! jsx-handler-names — enforce that JSX event handler props are wired
//! to identifiers prefixed with `handle`, `on`, or `toggle`.
//!
//! Why: consistent naming (`handleClick` for local handlers, `onClick`
//! for props forwarded from parents) makes intent obvious at the call
//! site and removes a class of "what does this function actually do"
//! ambiguity during review.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "jsx-handler-names",
    description: "JSX event handler props must reference handlers named `handle*`, `on*`, or `toggle*`.",
    remediation: "Rename the referenced function to start with `handle`, `on`, or `toggle`, or inline the handler.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/jsx-handler-names.md",
    ),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
