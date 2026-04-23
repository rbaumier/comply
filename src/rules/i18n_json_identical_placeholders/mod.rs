mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "i18n-json-identical-placeholders",
    description: "Translation has different placeholders than the base locale.",
    remediation: "Use the same placeholder names as the base locale to ensure variables are correctly substituted.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["i18n"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Json, Backend::Text(Box::new(text::Check)))],
    }
}
