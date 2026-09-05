//! vue-no-watch-reactive-property — flag `watch(state.prop, ...)` (value, not getter).
//!
//! ## Rationale
//!
//! `reactive()` and the props object are proxies that hand back the property's
//! own value. `watch(state.count, …)` therefore gives `watch()` the current
//! value, not a source it can track, so the watcher never fires again. The
//! getter form `watch(() => state.count, …)` re-reads the property on every
//! dependency change.
//!
//! ## What fires
//!
//! `watch(<obj>.<prop>, …)` where `<obj>` resolves to a reactive proxy AND the
//! read returns a plain value. The proxies: a binding built by Vue's own
//! `reactive()` / `shallowReactive()` (imported from `vue`, auto-injected, or
//! imported from another module that exports a `reactive()` binding), a Pinia
//! store instance, or the props object bound by `defineProps()` /
//! `withDefaults()`. Longer chains count for the deep receivers
//! (`watch(state.user.name, …)` on a `reactive()` proxy); `shallowReactive()`
//! only proves its own root-level properties.
//!
//! ## What's clean
//!
//! - `watch(() => state.count, …)` — the getter form.
//! - `watch(count, …)` — a `Ref` is a valid `watch` source on its own.
//! - `watch(ctx.modelValue, …)` where `ctx` comes from `inject()` or a
//!   composable — such context objects hold `Ref`s and `ComputedRef`s, and
//!   nothing proves the receiver is a proxy. A receiver that cannot be proven
//!   a proxy stays silent instead of being assumed reactive.
//! - `watch(state.filters, …)` on `reactive({ filters: { page: 1 } })` — a deep
//!   proxy converts every nesting level, so an object-valued property reads back
//!   as a nested proxy, which `watch()` traverses deeply.
//! - `watch(state.r, …)` on `shallowReactive({ r: ref(0) })`, and
//!   `watch(props.model, …)` under `defineProps<{ model: Ref<string> }>()` — a
//!   shallow proxy (the props object is one) stores values as-is, so a `Ref` it
//!   holds reads back as the `Ref` itself.
//!
//! ## Language coverage
//!
//! Vue SFC `<script>` / `<script setup>` blocks, extracted with tree-sitter-vue
//! and re-parsed with oxc for symbol resolution.

mod oxc_vue;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-no-watch-reactive-property",
    description: "`watch(state.prop, ...)` passes a snapshot — the watcher fires once then never again.",
    remediation: "Use a getter: `watch(() => state.prop, ...)`, or destructure with `toRefs`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["vue"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::TreeSitter(Box::new(oxc_vue::Check)))],
    }
}
