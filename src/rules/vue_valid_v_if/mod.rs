//! vue-valid-v-if — enforce valid `v-if` directive usage in Vue templates.
//!
//! A `v-if` directive must be a bare directive with a value expression: it
//! cannot carry an argument (`v-if:foo`) or modifiers (`v-if.bar`), must have a
//! value (`v-if="cond"`, not a bare `v-if`), and cannot sit on the same element
//! as a `v-else` or `v-else-if` directive.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-valid-v-if",
    description: "`v-if` must have a value and no argument, no modifiers, and no sibling `v-else`/`v-else-if` directive on the same element.",
    remediation: "Give `v-if` a value expression, drop any argument or modifier, and move `v-else`/`v-else-if` to a separate sibling element.",
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
