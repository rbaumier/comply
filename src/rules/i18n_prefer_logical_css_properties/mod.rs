//! i18n-prefer-logical-css-properties — physical properties break RTL.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "i18n-prefer-logical-css-properties",
    description: "Physical CSS properties break RTL layouts — use logical equivalents.",
    remediation: "Replace `margin-left` → `margin-inline-start`, `padding-right` → `padding-inline-end`, `text-align: left` → `text-align: start`, `border-left` → `border-inline-start`, etc.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["i18n", "css"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
