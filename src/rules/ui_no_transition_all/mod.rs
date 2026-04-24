//! ui-no-transition-all — forbid `transition: all` / `transition-property: all`.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-transition-all",
    description: "Using `transition: all` animates every changed property, causing jank and unintended motion.",
    remediation: "List properties explicitly: `transition: transform 200ms, opacity 200ms;`.",
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
