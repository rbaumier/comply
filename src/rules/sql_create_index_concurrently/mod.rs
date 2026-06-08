//! sql-create-index-concurrently

mod rust;
mod text;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-create-index-concurrently",
    description: "`CREATE INDEX` without `CONCURRENTLY` takes an `ACCESS EXCLUSIVE` lock, blocking all table access.",
    remediation: "Use `CREATE INDEX CONCURRENTLY` for production migrations. Run outside a transaction block.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql", "migrations"],

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

/// True if `text` contains a `CREATE INDEX` (or `CREATE UNIQUE INDEX`)
/// without the `CONCURRENTLY` keyword.
pub(super) fn is_blocking_create_index(text: &str) -> bool {
    let upper = text.to_ascii_uppercase();
    if upper.contains("CONCURRENTLY") {
        return false;
    }
    upper.contains("CREATE INDEX") || upper.contains("CREATE UNIQUE INDEX")
}
