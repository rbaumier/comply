//! jsx-handler-names — enforce that JSX event handler props are wired
//! to identifiers prefixed with `handle`, `on`, or `toggle`.
//!
//! Why: consistent naming (`handleClick` for local handlers, `onClick`
//! for props forwarded from parents) makes intent obvious at the call
//! site and removes a class of "what does this function actually do"
//! ambiguity during review.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

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
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
