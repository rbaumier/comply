//! Shared helpers for rules that detect SQL inside string literals.
//!
//! Several SQL-quality rules (`sql-no-between-timestamp`,
//! `sql-no-offset-pagination`, `sql-no-varchar`, `db-no-string-concat-sql`,
//! `no-sql-string-format`, …) all face the same upstream problem: they
//! need to identify which string literals in a TS / Rust file are
//! actually SQL queries, vs which are prose / config / paths / etc.
//!
//! The previous TextCheck-based approach scanned the whole file for
//! keywords and produced false positives on identifiers, comments,
//! and unrelated strings. The right move is to walk the AST for
//! string-literal nodes, extract their text, and ask "is this a
//! SQL query?" via a single shared heuristic — `is_sql_string`.
//!
//! ## SQL detection heuristic
//!
//! A string is treated as SQL if it contains BOTH:
//! - at least one DML keyword as a whole word (`SELECT`, `INSERT`,
//!   `UPDATE`, `DELETE`), and
//! - a clause keyword (`WHERE`, `FROM`).
//!
//! Both checks are case-insensitive and use word boundaries so
//! `selected_at`, `deleted_at`, `from_user`, `where_clause` don't
//! match. Requiring BOTH a DML and a clause keyword almost
//! eliminates false positives on prose containing the word "from"
//! or "select" in an English sentence.

/// Whole-word, case-insensitive substring check. `text` should
/// already be lowercase. `word` should be lowercase.
pub fn contains_word(text: &str, word: &str) -> bool {
    let bytes = text.as_bytes();
    let needle = word.as_bytes();
    if needle.is_empty() || bytes.len() < needle.len() {
        return false;
    }
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if bytes[i..i + needle.len()] == *needle {
            let before_ok = i == 0 || !is_ident_byte(bytes[i - 1]);
            let after_idx = i + needle.len();
            let after_ok = after_idx >= bytes.len() || !is_ident_byte(bytes[after_idx]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

pub(crate) fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// True when `path` sits inside a migration directory — any path
/// component named `migrations`, `migration`, or containing `migrate`.
pub fn is_migration_path(path: &std::path::Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy().to_ascii_lowercase();
        s == "migrations" || s == "migration" || s.contains("migrate")
    })
}

/// Returns true if `text` looks like a SQL query. Requires at least
/// one DML keyword AND a `WHERE` or `FROM` clause keyword. Uses
/// whole-word matching so identifiers containing the keywords don't
/// trigger.
pub fn is_sql_string(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let has_dml = ["select", "insert", "update", "delete"]
        .iter()
        .any(|kw| contains_word(&lower, kw));
    if !has_dml {
        return false;
    }
    contains_word(&lower, "where") || contains_word(&lower, "from")
}

/// Returns true if `text` looks like a SQL DDL statement (schema
/// management) — `CREATE TABLE`, `ALTER TABLE`, etc. Used by rules
/// that look for column type smells (`VARCHAR`, `TIMESTAMP` without
/// timezone, …) which only appear in DDL, never in DML.
///
/// Heuristic: requires `CREATE` or `ALTER` AND `TABLE` or `TYPE`,
/// both whole-word matched. This catches the common schema
/// statements while rejecting English prose and identifiers.
pub fn is_sql_ddl(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let has_verb = contains_word(&lower, "create") || contains_word(&lower, "alter");
    if !has_verb {
        return false;
    }
    contains_word(&lower, "table") || contains_word(&lower, "type")
}

/// Returns true if `lower_text` (already lowercase) contains `word`
/// (lowercase) at a word boundary AND the next non-whitespace
/// character is `(`. Use this to detect SQL function/type calls
/// like `VARCHAR(255)`, `DECIMAL(10, 2)`, etc., without matching
/// identifiers like `same_char(` or `bpchar_value`.
pub fn word_followed_by_open_paren(lower_text: &str, word: &str) -> bool {
    let bytes = lower_text.as_bytes();
    let needle = word.as_bytes();
    if needle.is_empty() || bytes.len() < needle.len() {
        return false;
    }
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if bytes[i..i + needle.len()] == *needle {
            let before_ok = i == 0 || !is_ident_byte(bytes[i - 1]);
            if before_ok {
                let mut j = i + needle.len();
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'(' {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

/// Tree-sitter node kinds that represent string literals in TS / TSX / JS.
/// Used by SQL rules to find candidate strings via
/// `walker::collect_nodes_of_kinds`.
pub const TS_STRING_KINDS: &[&str] = &["string", "template_string"];

/// Tree-sitter node kinds that represent string literals in Rust.
pub const RUST_STRING_KINDS: &[&str] = &["string_literal", "raw_string_literal"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_basic_select() {
        assert!(is_sql_string("SELECT * FROM users WHERE id = 1"));
    }

    #[test]
    fn detects_insert() {
        assert!(is_sql_string("INSERT INTO users (id) SELECT 1 FROM dual"));
    }

    #[test]
    fn rejects_prose_with_keywords_as_substrings() {
        // `selected` and `delivered_from` should NOT match.
        assert!(!is_sql_string("the user selected items delivered from store"));
    }

    #[test]
    fn rejects_dml_without_clause() {
        // SELECT alone, no FROM/WHERE.
        assert!(!is_sql_string("SELECT 1"));
    }

    #[test]
    fn rejects_clause_without_dml() {
        assert!(!is_sql_string("from start to end where they came from"));
    }

    #[test]
    fn rejects_identifier_with_keyword() {
        // `delete_user` and `where_clause` are identifiers, not keywords.
        assert!(!is_sql_string("function delete_user(where_clause) {}"));
    }

    #[test]
    fn case_insensitive() {
        assert!(is_sql_string("update users set x = 1 where id = 2"));
        assert!(is_sql_string("Update Users Set X = 1 Where Id = 2"));
    }

    #[test]
    fn contains_word_respects_underscores() {
        assert!(!contains_word("select_id", "select"));
        assert!(!contains_word("id_select", "select"));
        assert!(contains_word("select id", "select"));
    }

    #[test]
    fn detects_create_table_ddl() {
        assert!(is_sql_ddl("CREATE TABLE users (id INT)"));
    }

    #[test]
    fn detects_alter_table_ddl() {
        assert!(is_sql_ddl("ALTER TABLE users ADD COLUMN name TEXT"));
    }

    #[test]
    fn detects_create_type_ddl() {
        assert!(is_sql_ddl("CREATE TYPE status AS ENUM ('a', 'b')"));
    }

    #[test]
    fn rejects_dml_as_ddl() {
        assert!(!is_sql_ddl("SELECT * FROM users"));
    }

    #[test]
    fn word_followed_by_open_paren_matches_varchar() {
        assert!(word_followed_by_open_paren("name varchar(255)", "varchar"));
    }

    #[test]
    fn word_followed_by_open_paren_matches_with_space() {
        assert!(word_followed_by_open_paren("name varchar (255)", "varchar"));
    }

    #[test]
    fn word_followed_by_open_paren_rejects_identifier_prefix() {
        // `same_char(` should NOT match `char`.
        assert!(!word_followed_by_open_paren(
            "fn flags_negative_lookahead_same_char()",
            "char"
        ));
    }

    #[test]
    fn word_followed_by_open_paren_rejects_identifier_suffix() {
        // `varchar_value` should NOT match `varchar`.
        assert!(!word_followed_by_open_paren("varchar_value(arg)", "varchar"));
    }

}
