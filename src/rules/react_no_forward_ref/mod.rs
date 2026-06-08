//! react-no-forward-ref — flag `forwardRef(...)` calls.
//!
//! React 19 makes `ref` a regular prop on function components, so the
//! `forwardRef(...)` wrapper is no longer needed and is documented as
//! deprecated. Remove the wrapper and accept `ref` as a regular prop.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-forward-ref",
    description: "`forwardRef(...)` is deprecated in React 19 — accept `ref` as a regular prop.",
    remediation: "Remove the `forwardRef` wrapper and declare `ref` in the component props. \
                  React 19 forwards refs automatically to function components.",
    severity: Severity::Warning,
    doc_url: Some("https://react.dev/blog/2024/12/05/react-19#ref-as-a-prop"),
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
