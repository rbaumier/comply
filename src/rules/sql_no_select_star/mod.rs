//! sql-no-select-star

mod oxc_drizzle;
#[cfg(test)]
mod drizzle;
mod rust;
mod text;
mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-select-star",
    description: "`SELECT *` wastes bandwidth and prevents covering indexes.",
    remediation: "List columns explicitly: `SELECT id, name, email` instead of `SELECT *`. Explicit columns enable index-only scans and make the API contract visible.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_drizzle::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::Tsx,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Vue, Backend::Text(Box::new(text::Check))),
            (Language::Sql, Backend::Text(Box::new(text::Check))),
        ],
    }
}

/// True if `text` contains a `SELECT *` (case-insensitive), allowing
/// for one or two spaces between the keyword and the asterisk.
pub(super) fn contains_select_star(text: &str) -> bool {
    let upper = text.to_ascii_uppercase();
    upper.contains("SELECT *") || upper.contains("SELECT  *")
}
