//! use-qwik-method-usage — forbid Qwik `use*` hooks outside a `component$` call
//! or another `use*` hook.
//!
//! Qwik's reactive hooks (`useSignal`, `useStore`, `useTask$`, …) may only run
//! while a component or hook is being set up. Calling one anywhere else — module
//! scope, a plain function, an event handler, a callback — runs it outside any
//! reactive context, which is a runtime error in Qwik.
//!
//! A `use*` hook is an identifier call whose name starts with `use` followed by
//! an uppercase letter (`useSignal`, `useTask$`) AND whose binding is imported
//! from `@builder.io/qwik` or `qwik`. The import gate keeps the rule from firing
//! on same-named helpers in non-Qwik projects (React `useState`, custom `use*`).
//!
//! The call is allowed when its nearest enclosing function is either wrapped in a
//! `component$(...)` call (including an aliased import of `component$`) or is
//! itself a `use*`-named function (a custom hook like `useCounter`). Any other
//! enclosing context is flagged.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "use-qwik-method-usage",
    description: "Qwik `use*` hooks must run inside `component$` or another `use*` hook.",
    remediation: "Move the hook into a `component$(...)` callback or a `use*`-named hook.",
    severity: Severity::Error,
    doc_url: Some("https://biomejs.dev/linter/rules/use-qwik-method-usage/"),
    categories: &["correctness", "qwik"],

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
