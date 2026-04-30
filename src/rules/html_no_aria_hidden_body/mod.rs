//! html-no-aria-hidden-body
//!
//! Flags `<body aria-hidden="true">`. Hiding the entire body from
//! assistive technology leaves screen-reader users with no accessible
//! content.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "html-no-aria-hidden-body",
    description: "`aria-hidden=\"true\"` must not be applied to the `<body>` element.",
    remediation: "Remove `aria-hidden` from `<body>`; scope the attribute to the specific subtree you want to hide.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["a11y"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
