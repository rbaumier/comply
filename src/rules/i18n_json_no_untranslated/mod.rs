mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "i18n-json-no-untranslated",
    description: "Translation value is identical to the base locale — likely untranslated.",
    remediation: "Translate the value or confirm it should remain the same (brand names, etc.).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["i18n"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Json, Backend::Text(Box::new(text::Check)))],
    }
}
