//! i18n-json-valid-message-syntax — flag invalid ICU MessageFormat syntax
//! in JSON translation files.
//!
//! Targets JSON files under typical i18n locations (`locales/`, `i18n/`,
//! `translations/`) or with the suffix `*.locale.json` / `*.i18n.json`.
//! Each string value is validated for well-formed ICU MessageFormat:
//! balanced braces, valid `plural` / `select` bodies, and matching
//! argument syntax. Malformed messages crash i18n formatters at runtime
//! or silently render the raw template — both are user-visible bugs.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "i18n-json-valid-message-syntax",
    description: "Invalid ICU MessageFormat syntax in JSON translation files.",
    remediation: "Fix the ICU MessageFormat expression. Common mistakes: \
                  unbalanced braces, missing `other` branch in `plural` / \
                  `select`, or a keyword (`plural`, `select`, `selectordinal`) \
                  used without branches.",
    severity: Severity::Warning,
    doc_url: Some("https://formatjs.io/docs/core-concepts/icu-syntax/"),
    categories: &["i18n"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Json, Backend::Text(Box::new(text::Check)))],
    }
}
