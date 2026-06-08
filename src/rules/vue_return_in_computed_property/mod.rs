//! vue-return-in-computed-property — `computed()` callback must return a value.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-return-in-computed-property",
    description: "A `computed()` callback that never returns a value resolves to `undefined`.",
    remediation: "Return the derived value from the callback — `computed(() => a.value + b.value)` \
                  or, with a block body, an explicit `return`.",
    severity: Severity::Error,
    doc_url: Some("https://eslint.vuejs.org/rules/return-in-computed-property.html"),
    categories: &["vue"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}
