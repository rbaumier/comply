mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-no-v-html-unsafe",
    description: "`v-html` without sanitization is an XSS vector.",
    remediation: "Wrap the value in `DOMPurify.sanitize(...)` before binding with `v-html`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["vue", "security"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}
