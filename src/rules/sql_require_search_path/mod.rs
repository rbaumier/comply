//! sql-require-search-path

mod rust;
mod sql;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
pub(super) use crate::rules::sql_helpers::is_migration_path;

pub const META: RuleMeta = RuleMeta {
    id: "sql-require-search-path",
    description: "Migration files must set `search_path` or use schema-qualified identifiers.",
    remediation: "Start migrations with `SET search_path = pg_catalog, public;` or qualify every identifier (`public.user`, `pg_catalog.setval`). An attacker with CREATE on any schema in search_path can shadow functions.",
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
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::Tsx,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Sql, Backend::Text(Box::new(sql::Check))),
        ],
    }
}

pub(super) fn sql_creates_or_alters_table(sql: &str) -> bool {
    let upper = sql.to_ascii_uppercase();
    upper.contains("CREATE TABLE") || upper.contains("ALTER TABLE")
}

pub(super) fn sql_sets_search_path(sql: &str) -> bool {
    let upper = sql.to_ascii_uppercase();
    let compact: String = upper.chars().filter(|c| !c.is_whitespace()).collect();
    compact.contains("SETSEARCH_PATH=") || compact.contains("SETSEARCH_PATHTO")
}
