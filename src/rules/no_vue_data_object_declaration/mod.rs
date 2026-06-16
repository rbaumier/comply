//! no-vue-data-object-declaration — declare a Vue component's `data` as a function.
//!
//! ## Rationale
//!
//! Ported from Biome's `noVueDataObjectDeclaration`. A Vue component's `data`
//! option declared as an object literal is shared across every instance of the
//! component, so mutating one instance's state leaks into all the others.
//! Declaring `data` as a function (`data() { return { … } }`) gives each
//! instance its own object.
//!
//! ## What fires
//!
//! A `data` option whose value is an object literal (parentheses omitted), on a
//! component options object taken from:
//!
//! - `export default { … }`;
//! - `defineComponent(…)` — the last argument;
//! - `createApp(…)` — the first argument.
//!
//! ## What's clean
//!
//! - `data` declared as a method (`data() { … }`), a function expression, or an
//!   arrow (including `data: () => ({ … })`);
//! - a `data` object literal that is not the component's top-level `data` option
//!   (e.g. a local variable inside a method).
//!
//! ## Language coverage
//!
//! TypeScript / JavaScript / TSX via the oxc backend, and Vue SFC `<script>` /
//! `<script setup>` blocks (extracted with tree-sitter-vue, re-parsed with oxc).

mod oxc_typescript;
mod oxc_vue;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-vue-data-object-declaration",
    description: "A Vue component's `data` option was declared as an object instead of a function.",
    remediation: "Declare `data` as a function returning the object (`data() { return { … } }`) \
                  so each component instance gets its own state.",
    severity: Severity::Warning,
    doc_url: Some("https://biomejs.dev/linter/rules/no-vue-data-object-declaration/"),
    categories: &["vue"],

    skip_in_test_dir: false,
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
            (Language::Vue, Backend::TreeSitter(Box::new(oxc_vue::Check))),
        ],
    }
}
