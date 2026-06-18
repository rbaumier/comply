//! nuxt-no-v-html-in-server

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "nuxt-no-v-html-in-server",
    description: "`v-html` in an SSR-rendered component is an XSS vector when the value is not sanitized.",
    remediation: "Pass the value through DOMPurify (or a server-side sanitizer) before binding, or render structured content as components.",
    severity: Severity::Error,
    doc_url: Some("https://vuejs.org/api/built-in-directives.html#v-html"),
    categories: &["nuxt", "security"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
