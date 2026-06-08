//! react-no-render-return-value — flag capturing the return value of
//! `ReactDOM.render()`.
//!
//! Why: since React 16 the value returned by `ReactDOM.render()` is
//! unreliable and discouraged. Code that relies on it (assigning the
//! root component instance) breaks under concurrent rendering and
//! silently returns `null` in many cases.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-render-return-value",
    description: "Do not use the return value of `ReactDOM.render()`.",
    remediation: "Call `ReactDOM.render()` as a statement; attach refs via `ref` callbacks instead.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/no-render-return-value.md",
    ),
    categories: &["react"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
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
