//! sql-no-like-wildcard-prefix

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
    id: "sql-no-like-wildcard-prefix",
    description: "`LIKE '%...'` prevents index usage — use full-text search instead.",
    remediation: "Replace `LIKE '%term%'` with a TSVECTOR + GIN index and `@@` operator. Leading wildcards force a sequential scan on every row.",
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

/// True if `text` contains a `LIKE '%...` (leading-wildcard) pattern.
/// Matches both single and double quote variants, case-insensitive on
/// the keyword.
pub(super) fn has_leading_wildcard_like(text: &str) -> bool {
    text.contains("LIKE '%")
        || text.contains("like '%")
        || text.contains("LIKE \"%")
        || text.contains("like \"%")
}
