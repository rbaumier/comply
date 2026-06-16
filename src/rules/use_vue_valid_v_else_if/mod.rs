//! use-vue-valid-v-else-if — enforce valid `v-else-if` directive usage in Vue
//! templates.
//!
//! A `v-else-if` directive must be a bare directive with a value expression: it
//! cannot carry an argument (`v-else-if:foo`) or modifiers (`v-else-if.bar`),
//! must have a value (`v-else-if="cond"`, not a bare `v-else-if`), cannot sit on
//! the same element as a `v-if` or `v-else` directive, and must be on an element
//! whose immediately preceding sibling element carries a valid `v-if` or
//! `v-else-if` directive (comment and whitespace nodes between siblings are
//! skipped).

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "use-vue-valid-v-else-if",
    description: "`v-else-if` must have a value, no argument or modifiers, no sibling `v-if`/`v-else` on the same element, and a preceding sibling element with `v-if`/`v-else-if`.",
    remediation: "Give `v-else-if` a value expression, drop any argument or modifier, keep it on its own element, and place it after an element with `v-if`/`v-else-if`.",
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
