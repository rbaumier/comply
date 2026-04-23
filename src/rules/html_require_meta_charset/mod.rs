//! html-require-meta-charset — flag HTML documents missing a `<meta charset>`
//! (or legacy `<meta http-equiv="Content-Type">`) declaration.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "html-require-meta-charset",
    description: "HTML documents must declare a character encoding via `<meta charset>`.",
    remediation: "Add <meta charset=\"utf-8\"> to the head",
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
