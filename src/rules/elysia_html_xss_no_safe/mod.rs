//! elysia-html-xss-no-safe

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-html-xss-no-safe",
    description: "JSX expression interpolating user input without `safe` attribute — XSS vector.",
    remediation: "Add the `safe` attribute on the surrounding element so `@elysiajs/html` escapes the content.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
