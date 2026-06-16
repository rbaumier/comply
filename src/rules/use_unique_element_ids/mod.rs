//! use-unique-element-ids — flag a static string-literal `id` on a JSX element.
//!
//! React components can render more than once, so a hardcoded `id` produces
//! duplicate DOM ids at runtime. The fix is a generated id (`useId()`). This
//! ports Biome's `useUniqueElementIds`: it fires on a `JsxString`-valued `id`
//! attribute (`<div id="foo">`) and on a literal-valued `id` in a React
//! `createElement(tag, { id: "foo" })` call. Dynamic ids (`id={x}`) are fine.
//!
//! Distinct from `html-no-duplicate-id`, which is a Vue/HTML text check for the
//! *same* id appearing twice in one document; this rule flags a *single* static
//! id literal on any JSX element regardless of duplication.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "use-unique-element-ids",
    description: "Avoid a static string-literal `id` attribute on a JSX element.",
    remediation: "A reused component renders duplicate ids. Generate the id with \
                  React's `useId()` hook and pass it via `id={id}` instead of a \
                  hardcoded string.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    let backends = vec![
        (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
    ];
    RuleDef {
        meta: META,
        backends,
    }
}
