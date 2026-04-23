//! html-require-img-alt
//!
//! Flags `<img>` elements without an `alt` attribute. Narrower than
//! `a11y-alt-text` (which also covers `<area>` and `<input
//! type="image">`); useful as an HTML-level, cheaper-to-reason-about
//! check.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "html-require-img-alt",
    description: "`<img>` elements must declare an `alt` attribute.",
    remediation: "Add `alt=\"<description>\"` for meaningful images or `alt=\"\"` for decorative ones.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["a11y"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
