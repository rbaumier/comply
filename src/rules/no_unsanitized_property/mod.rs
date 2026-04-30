//! no-unsanitized-property — flag unsafe assignments to `innerHTML`,
//! `outerHTML`, or `srcdoc` where the right-hand side is not a static
//! string literal. Any non-literal value is a potential XSS vector.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-unsanitized-property",
    description: "Assigning a non-literal value to `innerHTML`, `outerHTML`, or `srcdoc` is an XSS vector.",
    remediation: "Use textContent, or sanitize HTML before assignment",
    severity: Severity::Error,
    doc_url: Some(
        "https://developer.mozilla.org/en-US/docs/Web/API/Element/innerHTML#security_considerations",
    ),
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
