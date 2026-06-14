//! react-display-name — flag anonymous React components exported without
//! a stable name.
//!
//! Why: anonymous components render as `<_>` or `<Unknown>` in React
//! DevTools and inside error boundaries. Giving every component a name
//! (either via `function Foo()` or `displayName`) makes profiling, error
//! stacks, and Fast Refresh boundaries intelligible.
//!
//! Files for non-React JSX frameworks (SolidJS, Vue, Preact, Qwik, Stencil)
//! are exempt: display names are a React DevTools / Fast Refresh concern, and
//! those frameworks use anonymous `export default function` route components in
//! file-based routing as a first-class pattern.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-display-name",
    description: "React components must have a display name.",
    remediation: "Name the function, assign it to a named variable before exporting, or set `displayName`.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/display-name.md",
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
