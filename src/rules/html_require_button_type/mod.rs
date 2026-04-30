//! html-require-button-type
//!
//! Flags `<button>` elements that do not declare an explicit `type`
//! attribute. Without `type`, browsers default to `submit` inside a
//! form, causing accidental submissions.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "html-require-button-type",
    description: "`<button>` must have an explicit `type` attribute.",
    remediation: "Add `type=\"button\"`, `type=\"submit\"`, or `type=\"reset\"` to the `<button>` element.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["a11y"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
