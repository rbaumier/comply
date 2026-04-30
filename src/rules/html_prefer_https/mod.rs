//! html-prefer-https

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "html-prefer-https",
    description: "HTML `href`, `src`, and `action` attributes should use `https://` instead of `http://`.",
    remediation: "Use https:// instead of http://",
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
