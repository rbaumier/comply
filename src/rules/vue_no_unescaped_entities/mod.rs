//! vue-no-unescaped-entities

mod vue;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "vue-no-unescaped-entities",
    description: "Unescaped entities in Vue template text can cause unexpected rendering.",
    remediation: "Replace the character with its HTML entity.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["vue"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::Vue, Backend::TreeSitter(Box::new(vue::Check))),
        ],
    }
}
