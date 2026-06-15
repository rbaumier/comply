//! no-solid-destructured-props — forbid destructuring a Solid component's props.
//!
//! In Solid, props are a reactive proxy: reading `props.foo` inside JSX tracks
//! the dependency, so the component re-runs that expression when `foo` changes.
//! Destructuring (`({ foo }) => ...`) reads every field once at call time,
//! freezing the values and breaking reactivity. Components must keep the single
//! `props` parameter intact and access fields with `props.foo`.
//!
//! A component is the arrow function assigned to a PascalCase variable
//! (`let Component = (props) => ...`) taking exactly one parameter. The rule
//! flags an empty destructuring `{}` outright, and otherwise flags each
//! destructured binding that is read inside a JSX expression attribute value
//! (`<div a={foo} />`).
//!
//! Solid-only: destructuring props is idiomatic in React, so the rule fires only
//! in files that belong to a SolidJS project (Solid import, `@jsxImportSource
//! solid-js` pragma, or a `solid-js` dependency).

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-solid-destructured-props",
    description: "Solid component props must not be destructured — it breaks reactivity.",
    remediation: "Keep the single `props` parameter and access fields with `props.foo`.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://biomejs.dev/linter/rules/no-solid-destructured-props/",
    ),
    categories: &["correctness", "solid"],

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
