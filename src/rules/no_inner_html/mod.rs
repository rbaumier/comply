//! no-inner-html

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-inner-html",
    description: "Avoid assigning to `.innerHTML` / `.outerHTML`.",
    remediation: "Use `textContent` for plain text or `appendChild` for nodes. If HTML is truly required, sanitize it with DOMPurify first — `innerHTML =` is a classic XSS sink.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
