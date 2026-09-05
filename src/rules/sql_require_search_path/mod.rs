//! sql-require-search-path
//!
//! A migration satisfies the rule either way its object names can stop
//! depending on the ambient path: by assigning `search_path` itself, or by
//! schema-qualifying every `CREATE`/`ALTER TABLE` target.
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
use crate::rules::sql_helpers::{
    DDL_MODIFIERS, find_word, is_ident_byte, strip_leading_word, strip_trailing_word,
};

pub const META: RuleMeta = RuleMeta {
    id: "sql-require-search-path",
    description: "Migration files must set `search_path` or schema-qualify their `CREATE`/`ALTER TABLE` targets.",
    remediation: "Start migrations with `SET LOCAL search_path = public;` (or your app schema): `LOCAL` reverts the path at COMMIT instead of leaving it on the pooled connection, and leaving `pg_catalog` unnamed keeps it searched first for built-ins while `current_schema()` stays writable — a leading `pg_catalog` makes unqualified `CREATE TABLE` fail. Alternatively schema-qualify every `CREATE`/`ALTER TABLE` target (`public.user`). An attacker with CREATE on any schema in search_path can shadow functions.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database", "sql"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

/// The single wording every backend emits. `SET LOCAL search_path = public;`
/// is the spelling that both survives the migration (it reverts at COMMIT
/// rather than staying on the pooled connection) and keeps `current_schema()`
/// writable, so unqualified `CREATE TABLE` still works.
pub(super) const MESSAGE: &str = "Migration must set `search_path` (`SET LOCAL search_path = public;`) or schema-qualify its `CREATE`/`ALTER TABLE` targets, to prevent identifier hijacking.";

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

/// Keywords Postgres allows between the `TABLE` keyword and the target name —
/// `CREATE TABLE IF NOT EXISTS t`, `ALTER TABLE IF EXISTS ONLY t`.
const TABLE_TARGET_PREFIXES: &[&str] = &["if", "not", "exists", "only"];

/// True if `sql` holds `CREATE TABLE` / `ALTER TABLE` DDL whose target name is
/// not schema-qualified.
///
/// An unqualified target is resolved through `search_path`, which is the
/// exposure this rule exists to close. A qualified one (`public."account"`)
/// names its schema outright and cannot be hijacked, so it needs no
/// `search_path` — that is the escape the rule's remediation offers. SQL that
/// mixes both still counts as exposed.
pub(super) fn has_search_path_dependent_ddl(sql: &str) -> bool {
    let lower = sql.to_ascii_lowercase();
    let mut from = 0;
    while let Some((start, end)) = find_word(&lower, "table", from) {
        from = end;
        if ddl_verb_precedes(&lower[..start]) && !target_is_qualified(&lower[end..]) {
            return true;
        }
    }
    false
}

/// True if `before` — the text leading up to a `TABLE` keyword — ends with the
/// `CREATE`/`ALTER` verb, across the modifier keywords Postgres allows between
/// verb and object (`CREATE UNLOGGED TABLE`, `CREATE GLOBAL TEMPORARY TABLE`).
fn ddl_verb_precedes(before: &str) -> bool {
    let mut before = before;
    loop {
        if strip_trailing_word(before, "create").is_some()
            || strip_trailing_word(before, "alter").is_some()
        {
            return true;
        }
        let Some(rest) = DDL_MODIFIERS
            .iter()
            .find_map(|modifier| strip_trailing_word(before, modifier))
        else {
            return false;
        };
        before = rest;
    }
}

/// True if the target name in `after` — the text following a `TABLE` keyword —
/// is schema-qualified, i.e. its first identifier is followed by a `.`. Reads
/// both the bare and the quoted identifier spellings (`public.t`, `"public".t`).
fn target_is_qualified(after: &str) -> bool {
    let mut rest = after.trim_start();
    while let Some(next) = TABLE_TARGET_PREFIXES
        .iter()
        .find_map(|keyword| strip_leading_word(rest, keyword))
    {
        rest = next.trim_start();
    }
    let after_name = match rest.strip_prefix('"') {
        Some(inner) => inner.split_once('"').map(|(_, tail)| tail),
        None => {
            let len = rest.bytes().take_while(|b| is_ident_byte(*b)).count();
            (len > 0).then(|| &rest[len..])
        }
    };
    after_name.is_some_and(|tail| tail.trim_start().starts_with('.'))
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
    fn schema_qualified_targets_need_no_search_path_issue_8503() {
        // A qualified target names its schema outright, so nothing resolves
        // through `search_path` — the escape the remediation advertises.
        assert!(!has_search_path_dependent_ddl(
            "ALTER TABLE public.\"objective\" ADD COLUMN \"unit\" text NOT NULL;"
        ));
        assert!(!has_search_path_dependent_ddl(
            "CREATE TABLE \"public\".\"article_group\" (\"id\" uuid PRIMARY KEY);"
        ));
        assert!(!has_search_path_dependent_ddl(
            "CREATE TABLE IF NOT EXISTS public.account (id INT);"
        ));
        assert!(!has_search_path_dependent_ddl(
            "ALTER TABLE ONLY public.account ADD COLUMN name TEXT;"
        ));
    }

    #[test]
    fn unqualified_targets_still_depend_on_search_path() {
        assert!(has_search_path_dependent_ddl("CREATE TABLE users (id INT);"));
        assert!(has_search_path_dependent_ddl(
            "ALTER TABLE \"objective\" ADD COLUMN \"unit\" text;"
        ));
        assert!(has_search_path_dependent_ddl(
            "CREATE UNLOGGED TABLE cache (k TEXT);"
        ));
        // Whitespace between verb and object, and the `IF NOT EXISTS` prefix,
        // must not hide an unqualified target.
        assert!(has_search_path_dependent_ddl(
            "CREATE\n  TABLE IF NOT EXISTS users (id INT);"
        ));
        // One qualified statement does not cover an unqualified sibling.
        assert!(has_search_path_dependent_ddl(
            "CREATE TABLE public.a (id INT);\nCREATE TABLE b (id INT);"
        ));
    }

    #[test]
    fn ignores_statements_that_are_not_create_or_alter_table() {
        assert!(!has_search_path_dependent_ddl(
            "CREATE UNIQUE INDEX idx ON account (name);"
        ));
        assert!(!has_search_path_dependent_ddl("SELECT * FROM account;"));
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
