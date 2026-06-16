//! no-vue-reserved-keys — don't use Vue-reserved keys in component options.
//!
//! ## Rationale
//!
//! Ported from Biome's `noVueReservedKeys`. Vue reserves a set of `$`-prefixed
//! instance properties (`$el`, `$emit`, `$props`, …) and, in `data`, the `_`
//! prefix for its internal reactivity bookkeeping. Declaring a `data`,
//! `computed`, `methods`, or `props` entry with one of these names shadows the
//! framework member: the option is silently ignored or the instance breaks.
//!
//! ## What fires
//!
//! Inside a Vue component options object (`export default { … }`) or a
//! `<script setup>` `defineProps`:
//!
//! - a `$`-reserved name as a key of `data` / `computed` / `methods` / `props`,
//!   of a `data` / `asyncData` return object, or of `defineProps(...)` /
//!   `defineProps<…>()`;
//! - a `_`-prefixed key of a `data` / `asyncData` return object (the `_` prefix
//!   is reserved only for `data`, not for `computed` / `methods` / `setup`).
//!
//! ## What's clean
//!
//! - non-reserved names (`message`, `count`, `displayMessage`);
//! - a `_`-prefixed name in `methods` / `computed` / a `setup` return — the `_`
//!   prefix is reserved only in `data`.
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
    id: "no-vue-reserved-keys",
    description: "A Vue-reserved key was used in a component's data, computed, methods, or props.",
    remediation: "Rename the key: Vue reserves the `$` prefix (e.g. `$el`, `$emit`) and the `_` \
                  prefix in `data` for internal use.",
    severity: Severity::Warning,
    doc_url: Some("https://biomejs.dev/linter/rules/no-vue-reserved-keys/"),
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
