//! i18n-prefer-logical-css-properties — physical properties break RTL.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
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
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
