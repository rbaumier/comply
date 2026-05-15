//! vue-no-async-in-computed-properties — `computed(async () => ...)`.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-no-async-in-computed-properties",
    description: "Vue `computed()` getters must be synchronous — an async getter returns a Promise that the template renders as `[object Promise]`.",
    remediation: "Move the async work to a `watch` or an action and store the resolved value in a ref. The `computed` should derive from synchronous state.",
    severity: Severity::Error,
    doc_url: Some("https://eslint.vuejs.org/rules/no-async-in-computed-properties.html"),
    categories: &["vue"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}
