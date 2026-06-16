//! use-vue-valid-v-bind — enforce valid `v-bind` directive usage in Vue templates.
//!
//! Covers both the longhand `v-bind` directive and its `:` shorthand. A binding
//! is reported when it has no value (`v-bind:foo`, `:foo`, `v-bind.prop`) or
//! carries a modifier outside the allowed set (`prop`, `camel`, `sync`, `attr`).
//! Argument-less object bindings with a value (`v-bind="props"`) are valid.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "use-vue-valid-v-bind",
    description: "`v-bind` bindings must have a value and only use the `prop`, `camel`, `sync`, or `attr` modifiers.",
    remediation: "Give the `v-bind` directive a value and drop any modifier outside `prop`, `camel`, `sync`, and `attr`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["vue"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
