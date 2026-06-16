//! use-solid-for-component — prefer Solid's `<For>` over `array.map()` in JSX.
//!
//! In Solid, rendering a list with `{items.map(item => <li />)}` recreates every
//! DOM element on each update, because `map` produces a fresh array of elements
//! that Solid cannot reconcile. Solid's `<For>` component keys rows by reference
//! and reuses the existing DOM, so only changed rows re-render.
//!
//! The rule flags a `.map()` call that is the direct expression of a JSX child
//! container — `{items.map(...)}` nested as a child of a JSX element or fragment
//! — taking exactly one argument (the callback). A `.map()` outside JSX, in a JSX
//! attribute value, or with a different arity is left alone.
//!
//! Solid-only: `array.map()` in JSX is idiomatic in React, so the rule fires only
//! in files that belong to a SolidJS project (Solid import, `@jsxImportSource
//! solid-js` pragma, or a `solid-js` dependency).

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "use-solid-for-component",
    description: "Prefer Solid's `<For>` component over `array.map()` to render lists in JSX.",
    remediation: "Replace `{items.map(item => <li />)}` with `<For each={items}>{item => <li />}</For>`.",
    severity: Severity::Warning,
    doc_url: Some("https://biomejs.dev/linter/rules/use-solid-for-component/"),
    categories: &["performance", "solid"],

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
