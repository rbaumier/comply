//! no-sql-string-format

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{Language, RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-sql-string-format",
    description: "SQL queries built with template literals or string concatenation are vulnerable to injection.",
    remediation: "Use parameterized queries or prepared statements instead of interpolating values into SQL strings.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    let mut backends: Vec<(Language, Backend)> = TS_FAMILY
        .iter()
        .map(|&lang| (lang, Backend::TreeSitter(Box::new(typescript::Check))))
        .collect();
    backends.push((Language::Rust, Backend::TreeSitter(Box::new(rust::Check))));
    RuleDef {
        meta: META,
        backends,
    }
}
