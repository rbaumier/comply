//! vue-prefer-shorthand-v-on — prefer the `@` shorthand over longhand
//! `v-on:` in Vue templates.
//!
//! An event binding written `v-on:click="onClick"` (or `v-on:[dyn]="onClick"`)
//! is flagged in favor of the `@click="onClick"` shorthand. Argument-less
//! `v-on="obj"` (object syntax) has no shorthand form and is left alone.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-prefer-shorthand-v-on",
    description: "Event bindings should use the `@` shorthand instead of longhand `v-on:`.",
    remediation: "Replace `v-on:click=\"onClick\"` with `@click=\"onClick\"`.",
    severity: Severity::Warning,
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
