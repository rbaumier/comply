//! a11y-img-redundant-alt

mod oxc_typescript;
#[cfg(test)]
mod react;
mod vue;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-img-redundant-alt",
    description: "`alt` text should not contain redundant words like \"image\", \"picture\", or \"photo\".",
    remediation: "Describe the image content instead of stating that it is an image. Remove words like \"image\", \"picture\", or \"photo\" from the `alt` attribute.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Vue, Backend::Text(Box::new(vue::Check))),
        ],
    }
}
