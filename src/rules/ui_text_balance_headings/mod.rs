//! ui-text-balance-headings — `h1`-`h6` should set `text-wrap: balance`.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-text-balance-headings",
    description: "Heading selectors (h1-h6) should declare `text-wrap: balance` to avoid orphan words.",
    remediation: "Add `text-wrap: balance;` to the heading rule.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Css, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
