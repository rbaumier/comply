mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "css-outline-none-needs-focus",
    description: "`outline: none` or `outline: 0` outside of `:focus` rules removes the keyboard focus indicator, harming accessibility.",
    remediation: "Only use `outline: none` inside `:focus` rules, and provide a visible alternative focus style.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["css", "a11y"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Css, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
