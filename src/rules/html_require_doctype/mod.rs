//! html-require-doctype — flag `.html` files missing a `<!DOCTYPE html>` declaration.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "html-require-doctype",
    description: "HTML files must start with a `<!DOCTYPE html>` declaration.",
    remediation: "Add <!DOCTYPE html> at the beginning of the file",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["html"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}
