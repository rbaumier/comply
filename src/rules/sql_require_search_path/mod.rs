//! sql-require-search-path
//!
//! `search_path` is a PostgreSQL concept, so a migration file whose SQL is
//! ClickHouse DDL (`is_clickhouse_ddl`) is skipped entirely.

mod rust;
mod sql;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
pub(super) use crate::rules::sql_helpers::is_migration_path;
use crate::rules::sql_helpers::{find_word, strip_leading_word, strip_trailing_word};

pub const META: RuleMeta = RuleMeta {
    id: "sql-require-search-path",
    description: "Migration files must set `search_path` or use schema-qualified identifiers.",
    remediation: "Start migrations with `SET search_path = pg_catalog, public;` or qualify every identifier (`public.user`, `pg_catalog.setval`). An attacker with CREATE on any schema in search_path can shadow functions.",
    severity: Severity::Error,
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

/// True if `sql` assigns `search_path`.
///
/// Postgres spells the statement `SET [ SESSION | LOCAL ] search_path { = | TO }
/// value`. The optional scope keyword is part of that grammar and both
/// spellings satisfy this rule's premise: `SET LOCAL` binds the setting to the
/// enclosing transaction and reverts it at `COMMIT`, so it covers the migration
/// without leaving the path on the pooled connection afterwards.
pub(super) fn sql_sets_search_path(sql: &str) -> bool {
    let lower = sql.to_ascii_lowercase();
    let mut from = 0;
    while let Some((start, end)) = find_word(&lower, "search_path", from) {
        from = end;
        let before = &lower[..start];
        let before = strip_trailing_word(before, "local")
            .or_else(|| strip_trailing_word(before, "session"))
            .unwrap_or(before);
        if strip_trailing_word(before, "set").is_none() {
            continue;
        }
        let after = lower[end..].trim_start();
        if after.starts_with('=') || strip_leading_word(after, "to").is_some() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_every_scope_keyword_of_the_set_grammar_issue_8512() {
        // `SET LOCAL` is the transaction-scoped spelling: it reverts at COMMIT
        // instead of riding the pooled connection into later statements, so a
        // migration that uses it is covered for the whole of its transaction.
        assert!(sql_sets_search_path("SET LOCAL search_path = pg_catalog, public;"));
        assert!(sql_sets_search_path("SET LOCAL search_path = public, pg_catalog;"));
        assert!(sql_sets_search_path("SET LOCAL search_path TO pg_catalog, public;"));
        assert!(sql_sets_search_path("SET SESSION search_path = public;"));
        assert!(sql_sets_search_path("set local search_path to public;"));
        // The bare spelling keeps working.
        assert!(sql_sets_search_path("SET search_path = pg_catalog, public;"));
        assert!(sql_sets_search_path("SET search_path TO public;"));
    }

    #[test]
    fn rejects_mentions_that_are_not_an_assignment() {
        // Reading the path is not setting it.
        assert!(!sql_sets_search_path("SELECT current_setting('search_path');"));
        // A different GUC, and a bare identifier with no SET verb.
        assert!(!sql_sets_search_path("SET LOCAL lock_timeout = '5s';"));
        assert!(!sql_sets_search_path("-- restore search_path afterwards"));
        // `RESET` clears the path rather than pinning it to a known value.
        assert!(!sql_sets_search_path("RESET search_path;"));
    }
}
