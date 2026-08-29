//! react-jsx-returning-function-is-component — a JSX-returning function is a component.
//! It must be named PascalCase and rendered as `<Name />`.
//! It must never be invoked as a plain function.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-returning-function-is-component",
    description: "Function returning JSX invoked as a function — name it PascalCase and render it as `<Name />`.",
    remediation: "A function that returns JSX is a component. Rename `renderBody` to `Body` and \
                  write `<Body />` instead of `{renderBody()}`. A plain call is inlined into the \
                  caller's element tree: the returned nodes get no fiber of their own, so they \
                  cannot hold hooks or state, and React cannot key, memoize or profile them.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["react"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
