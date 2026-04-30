//! html-require-closing-tags — flag non-void HTML tags that aren't closed.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "html-require-closing-tags",
    description: "Non-void HTML tags must be closed with a matching closing tag.",
    remediation: "Close HTML tag properly",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["html"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}
