//! migration-needs-lock-timeout
//!
//! Scoped to migration files only (`**/migrations/**`). For `.sql` files
//! the raw content is checked directly; for TS/Rust files, string literals
//! containing DDL are checked. The path filter is the primary guard —
//! `contains_ddl` only needs to identify DDL statements, not distinguish
//! SQL from prose.

mod rust;
mod sql;
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::sql_helpers::{contains_word, is_migration_path};
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "migration-needs-lock-timeout",
    description: "DDL migration without `SET lock_timeout` risks write queue pileups.",
    remediation: "Add `SET lock_timeout = '5s';` at the top of every DDL migration. Without it, an ALTER TABLE on a busy table queues all writes behind the lock indefinitely.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "migrations"],
};

pub fn register() -> RuleDef {
    let mut def = crate::register_ts_family_with_rust!(META, typescript, rust);
    def.backends
        .push((Language::Sql, Backend::Text(Box::new(sql::Check))));
    def
}

pub(super) fn contains_ddl(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    (contains_word(&lower, "alter") && contains_word(&lower, "table"))
        || (contains_word(&lower, "create") && contains_word(&lower, "index"))
        || (contains_word(&lower, "drop") && contains_word(&lower, "index"))
        || (contains_word(&lower, "add") && contains_word(&lower, "constraint"))
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
        assert!(!declares_lock_timeout("ALTER TABLE users ADD COLUMN age INT"));
    }
}
