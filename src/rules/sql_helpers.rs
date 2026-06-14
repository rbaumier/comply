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

/// DDL keywords that may legally sit between the verb (`CREATE`/`ALTER`)
/// and the object (`TABLE`/`TYPE`) — e.g. `CREATE OR REPLACE TYPE`,
/// `CREATE GLOBAL TEMPORARY TABLE`, `CREATE UNLOGGED TABLE`.
const DDL_MODIFIERS: &[&str] = &[
    "or", "replace", "temp", "temporary", "global", "local", "unlogged",
];

/// Returns true if `text` looks like a SQL DDL statement (schema
/// management) — `CREATE TABLE`, `ALTER TABLE`, etc. Used by rules
/// that look for column type smells (`VARCHAR`, `TIMESTAMP` without
/// timezone, …) which only appear in DDL, never in DML.
///
/// Heuristic: a whole-word `CREATE` or `ALTER` verb, followed by zero
/// or more DDL modifier keywords (`OR REPLACE`, `TEMPORARY`, …), with
/// the very next word being `TABLE` or `TYPE`. Requiring adjacency
/// (modulo modifiers) rejects English prose like "create a link type"
/// where arbitrary words separate the verb from the object.
pub fn is_sql_ddl(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .collect();
    for (i, w) in words.iter().enumerate() {
        if *w != "create" && *w != "alter" {
            continue;
        }
        // Skip DDL modifier keywords, then require the object keyword next.
        let mut j = i + 1;
        while j < words.len() && DDL_MODIFIERS.contains(&words[j]) {
            j += 1;
        }
        if j < words.len() && (words[j] == "table" || words[j] == "type") {
            return true;
        }
    }
    false
}

/// DDL verb/object/target triples that form a complete statement. Each
/// entry is `(verb, object)`; a complete statement names the target
/// object after `object` (e.g. `ALTER TABLE users …`, `CREATE INDEX idx …`).
const DDL_STATEMENT_SHAPES: &[(&str, &str)] = &[
    ("alter", "table"),
    ("create", "index"),
    ("drop", "index"),
    ("add", "constraint"),
];

/// Returns true if `text` contains a *complete* DDL statement, i.e. a
/// `verb object target …` shape where a target name follows the object
/// keyword.
///
/// This distinguishes a real migration statement (`ALTER TABLE users ADD
/// COLUMN age INT`) from a query-builder fragment (`query_builder
/// .push_sql("ALTER TABLE ")`), where the literal holds only the keyword
/// prefix with no target — there is no standalone statement to which a
/// `SET lock_timeout` could be attached.
///
/// Heuristic: a whole-word `verb` immediately followed by its `object`
/// keyword, immediately followed by at least one more word (the target).
/// Adjacency rejects English prose, and requiring the trailing target
/// rejects builder fragments.
pub fn contains_complete_ddl_statement(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .collect();
    DDL_STATEMENT_SHAPES.iter().any(|(verb, object)| {
        words.windows(3).any(|w| w[0] == *verb && w[1] == *object)
    })
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
        assert!(!is_sql_string(
            "the user selected items delivered from store"
        ));
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
    fn rejects_prose_create_with_distant_type_issue_1003() {
        // arktype ark/docs/.../type.ts: English JSDoc "Create ... Type" sharing a
        // string with TS `type` keywords must NOT be treated as DDL.
        assert!(!is_sql_ddl(
            "Create a copy of this `Type` with only the specified keys"
        ));
        assert!(!is_sql_ddl("create a link type")); // empirically the closest phrase (gap 2)
        assert!(!is_sql_ddl("you can create a custom type here"));
    }

    #[test]
    fn accepts_ddl_with_modifier_keywords() {
        assert!(is_sql_ddl("CREATE TEMPORARY TABLE t (x INT)"));
        assert!(is_sql_ddl("CREATE OR REPLACE TYPE status AS ENUM ('a')"));
        assert!(is_sql_ddl("CREATE GLOBAL TEMPORARY TABLE t (x INT)"));
        assert!(is_sql_ddl("CREATE UNLOGGED TABLE t (x INT)"));
    }

    #[test]
    fn complete_ddl_statement_accepts_full_alter_table() {
        assert!(contains_complete_ddl_statement(
            "ALTER TABLE users ADD COLUMN age INT"
        ));
        assert!(contains_complete_ddl_statement(
            "CREATE INDEX idx_users_age ON users(age)"
        ));
        assert!(contains_complete_ddl_statement("DROP INDEX idx_users_age"));
        assert!(contains_complete_ddl_statement(
            "ALTER TABLE users ADD CONSTRAINT fk FOREIGN KEY (id)"
        ));
    }

    #[test]
    fn complete_ddl_statement_rejects_builder_fragments_issue_1498() {
        // diesel diff_schema.rs query-builder fragments: the keyword prefix
        // is pushed alone, with no target name — not a standalone statement.
        assert!(!contains_complete_ddl_statement("ALTER TABLE "));
        assert!(!contains_complete_ddl_statement(" ADD COLUMN "));
        assert!(!contains_complete_ddl_statement(" DROP COLUMN "));
        assert!(!contains_complete_ddl_statement(" ADD CONSTRAINT "));
        assert!(!contains_complete_ddl_statement("CREATE INDEX "));
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
        assert!(!word_followed_by_open_paren(
            "varchar_value(arg)",
            "varchar"
        ));
    }
}
