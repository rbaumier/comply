//! vue-no-comment-textnodes

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "vue-no-comment-textnodes",
    description: "JS-style comments in Vue template text are rendered as visible text.",
    remediation: "Use `<!-- comment -->` for HTML comments in Vue templates.",
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
