//! vue-valid-v-for — enforce valid `v-for` directive usage in Vue templates.
//!
//! A `v-for` directive cannot carry an argument (`v-for:foo`) or modifiers
//! (`v-for.bar`), must have a value expression, and its secondary/tertiary tuple
//! aliases must be plain identifiers. A custom component rendered with `v-for`
//! requires a `:key` binding, and any `:key` present must reference one of the
//! variables introduced by the `v-for` binding.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-valid-v-for",
    description: "`v-for` must have a value, no argument or modifiers, identifier-only secondary aliases, and a `:key` that uses its iteration variables (required on custom components).",
    remediation: "Give `v-for` a `alias in iterable` value with no argument or modifier, keep extra aliases as plain identifiers, and add a `:key` referencing an iteration variable.",
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
