//! vue-valid-v-text — enforce valid `v-text` directive usage in Vue templates.
//!
//! A `v-text` directive must be a bare directive with a non-empty value
//! expression: it cannot carry an argument (`v-text:foo`) or modifiers
//! (`v-text.bar`), and must have a value (`v-text="msg"`, not a bare `v-text`,
//! an empty `v-text=""`, or a whitespace-only `v-text="   "`).

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-valid-v-text",
    description: "`v-text` must have a value and no argument or modifiers.",
    remediation: "Give `v-text` a value expression (e.g. `v-text=\"msg\"`) and drop any argument or modifier.",
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
