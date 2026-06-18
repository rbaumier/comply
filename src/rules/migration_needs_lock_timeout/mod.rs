//! migration-needs-lock-timeout
//!
//! Scoped to migration files only (`**/migrations/**`) and skipped in test
//! dirs, where DDL strings are inline snapshots/assertions of generated
//! migration output, not executed migrations. For `.sql` files the raw
//! content is checked directly; for TS/Rust files, string literals
//! containing a complete DDL statement are checked. `contains_ddl`
//! requires a `verb object target` shape so query-builder fragments
//! (`push_sql("ALTER TABLE ")`) — which have nowhere to attach a lock
//! timeout — are not flagged.

mod oxc_typescript;
mod rust;
mod sql;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::sql_helpers::{contains_complete_ddl_statement, contains_word};

pub const META: RuleMeta = RuleMeta {
    id: "migration-needs-lock-timeout",
    description: "DDL migration without `SET lock_timeout` risks write queue pileups.",
    remediation: "Add `SET lock_timeout = '5s';` at the top of every DDL migration. Without it, an ALTER TABLE on a busy table queues all writes behind the lock indefinitely.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "migrations"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Sql, Backend::Text(Box::new(sql::Check))),
        ],
    }
}

/// True when `text` holds a *complete* DDL statement that warrants a
/// `SET lock_timeout`. Requires a `verb object target …` shape so
/// query-builder fragments (`push_sql("ALTER TABLE ")`), which carry
/// only the keyword prefix and have nowhere to attach a lock timeout,
/// are not flagged.
pub(super) fn contains_ddl(text: &str) -> bool {
    contains_complete_ddl_statement(text)
}

pub(super) fn declares_lock_timeout(text: &str) -> bool {
    contains_word(&text.to_ascii_lowercase(), "lock_timeout")
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn detects_alter_table() {
        assert!(contains_ddl("ALTER TABLE users ADD COLUMN age INT"));
    }

    #[test]
    fn detects_create_index() {
        assert!(contains_ddl("CREATE INDEX idx_users_age ON users(age)"));
    }

    #[test]
    fn rejects_unrelated_text() {
        assert!(!contains_ddl("SELECT * FROM users"));
    }

    #[test]
    fn detects_lock_timeout_declaration() {
        assert!(declares_lock_timeout("SET lock_timeout = '5s';"));
    }

    #[test]
    fn rejects_non_lock_timeout() {
        assert!(!declares_lock_timeout(
            "ALTER TABLE users ADD COLUMN age INT"
        ));
    }
}
