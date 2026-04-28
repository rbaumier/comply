//! vue-no-duplicate-props

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "vue-no-duplicate-props",
    description: "Duplicate attributes on a Vue template element — the last one silently wins.",
    remediation: "Remove the duplicate attribute.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["vue"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::Vue, Backend::Text(Box::new(text::Check))),
        ],
    }
}
