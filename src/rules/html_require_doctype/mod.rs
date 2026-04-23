//! html-require-doctype — flag `.html` files missing a `<!DOCTYPE html>` declaration.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "html-require-doctype",
    description: "HTML files must start with a `<!DOCTYPE html>` declaration.",
    remediation: "Add <!DOCTYPE html> at the beginning of the file",
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
