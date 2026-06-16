//! no-vue-reserved-props — don't declare Vue-reserved names as component props.
//!
//! ## Rationale
//!
//! Ported from Biome's `noVueReservedProps`. Vue reserves the prop names `key`
//! and `ref` for its own template binding. Declaring either as a component prop
//! shadows the framework attribute: the value never reaches the child and the
//! built-in behaviour breaks.
//!
//! ## What fires
//!
//! A `key` or `ref` prop declared in any of:
//!
//! - the `props` option of `export default { … }` (array form `props: ['key']`
//!   or object form `props: { key: … }`), optionally wrapped in
//!   `defineComponent(…)` / `Vue.extend(…)`, including the
//!   `defineComponent(setup, { props: … })` two-argument form;
//! - a `createApp({ props: … })` root-component options object;
//! - `<script setup>` `defineProps([...])` / `defineProps({...})` /
//!   `defineProps<{ … }>()` (inline type literal, `interface`, or `type` alias).
//!
//! ## What's clean
//!
//! - non-reserved prop names (`foo`, `message`);
//! - `key` / `ref` used anywhere other than a prop declaration (e.g. `data`).
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
    id: "no-vue-reserved-props",
    description: "A Vue-reserved name (`key` or `ref`) was declared as a component prop.",
    remediation: "Rename the prop: Vue reserves `key` and `ref` for template binding, so they \
                  cannot be used as prop names.",
    severity: Severity::Warning,
    doc_url: Some("https://biomejs.dev/linter/rules/no-vue-reserved-props/"),
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
