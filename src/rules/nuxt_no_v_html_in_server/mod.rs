//! nuxt-no-v-html-in-server

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "nuxt-no-v-html-in-server",
    description: "`v-html` in an SSR-rendered component is an XSS vector when the value is not sanitized.",
    remediation: "Pass the value through DOMPurify (or a server-side sanitizer) before binding, or render structured content as components.",
    severity: Severity::Error,
    doc_url: Some("https://vuejs.org/api/built-in-directives.html#v-html"),
    categories: &["nuxt", "security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
