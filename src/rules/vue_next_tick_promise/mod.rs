//! vue-next-tick-promise — use the Promise form of Vue `nextTick`, not a callback.
//!
//! ## Rationale
//!
//! Ported from Biome's `useVueNextTickPromise`. Vue's `nextTick` returns a
//! Promise that resolves after the next DOM flush. Passing a callback
//! (`nextTick(() => { … })`) is the legacy Vue 2 shape; the Promise form
//! (`await nextTick()` / `nextTick().then(…)`) composes with async control
//! flow and surfaces rejections.
//!
//! ## What fires
//!
//! A call whose **first argument is a function expression** (arrow or
//! `function`) and whose callee is Vue's `nextTick`:
//!
//! - `nextTick(cb)` — `nextTick` is a named import from `vue` (alias-aware:
//!   `import { nextTick as nt } from 'vue'; nt(cb)` fires too).
//! - `Vue.nextTick(cb)` — member access on the `Vue` namespace global, or on a
//!   `* as Vue` namespace import from `vue`.
//! - `this.$nextTick(cb)` — the Options-API instance method.
//!
//! ## What's clean
//!
//! - `await nextTick()` / `nextTick().then(cb)` / `Vue.nextTick().then(cb)` —
//!   the Promise form (no callback argument).
//! - `nextTick("not a callback")` — first argument is not a function.
//! - `localNextTick(cb)` — `localNextTick` is a local function, not a vue
//!   import.
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
    id: "vue-next-tick-promise",
    description: "Vue `nextTick` was called with a callback instead of using its returned Promise.",
    remediation: "Drop the callback and await the Promise: `await nextTick()` \
                  (or `nextTick().then(…)`).",
    severity: Severity::Warning,
    doc_url: Some("https://biomejs.dev/linter/rules/use-vue-next-tick-promise/"),
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
