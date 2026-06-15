//! vue-valid-v-cloak — enforce valid `v-cloak` directive usage in Vue templates.
//!
//! `v-cloak` is a standalone boolean-like directive: it must not carry an
//! argument (`v-cloak:foo`), modifiers (`v-cloak.bar`), or a value
//! (`v-cloak="x"`). Only the bare `v-cloak` form is valid.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-valid-v-cloak",
    description: "`v-cloak` must be a bare directive with no argument, no modifiers, and no value.",
    remediation: "Use `v-cloak` on its own, dropping any argument, modifier, or value.",
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
