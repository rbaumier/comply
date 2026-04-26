//! migration-needs-lock-timeout
//!
//! Walks string-literal AST nodes for DDL statements that lack a
//! `SET lock_timeout` declaration. Operating on string literals only
//! (TS `string` / `template_string`, Rust `string_literal` /
//! `raw_string_literal`) avoids matching DDL keywords appearing in
//! comments, identifiers, or English prose.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::sql_helpers::contains_word;
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
    crate::register_ts_family_with_rust!(META, typescript, rust)
}

/// Returns true if `text` contains a DDL phrase: `ALTER TABLE`,
/// `CREATE INDEX`, `DROP INDEX`, or `ADD CONSTRAINT`. Each phrase is
/// matched as two whole words so identifiers can't trigger.
pub(super) fn contains_ddl(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    (contains_word(&lower, "alter") && contains_word(&lower, "table"))
        || (contains_word(&lower, "create") && contains_word(&lower, "index"))
        || (contains_word(&lower, "drop") && contains_word(&lower, "index"))
        || (contains_word(&lower, "add") && contains_word(&lower, "constraint"))
}

/// Returns true if `text` contains `lock_timeout` (whole-word, case
/// insensitive) — the marker that the migration already declared its
/// lock timeout, regardless of the value or quoting style.
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
    fn rejects_identifier_with_alter_substring() {
        assert!(!contains_ddl("alter_table_handler"));
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
