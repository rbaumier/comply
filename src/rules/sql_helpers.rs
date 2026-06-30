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
//! A string is treated as SQL only if it contains a recognized DML
//! statement shape, i.e. a verb followed (in token order) by its
//! mandatory companion clause:
//! - `SELECT` … `FROM`
//! - `DELETE` … `FROM`
//! - `INSERT` … `INTO`
//! - `UPDATE` … `SET`
//!
//! Matching is case-insensitive and word-boundary-aware (so
//! `selected_at`, `deleted_at`, `from_user`, `where_clause` don't
//! match). Requiring the verb and its companion to appear in the
//! correct order distinguishes real SQL from prose or generated
//! code that merely mentions individual keywords — e.g. "Would
//! update X from Y" (`update` with no `SET`) or Prisma JSDoc
//! containing `update`/`where`/`create` API method names.
//!
//! The text *between* the verb and its clause must also be shaped like
//! a SQL projection/target rather than English prose: a column list,
//! qualified name, function call or single column reference — never a
//! run of free-form words. This rejects sentences like "Select specific
//! fields to fetch from the model", where `select … from` appear in
//! clause order but the words between them are prose, not a projection.

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

/// True when `path` sits inside a migration directory — a path
/// component named exactly `migrations`, `migration`, or `migrate`.
/// An exact component match (not a substring) so a migration-framework
/// package whose own directory name merely contains `migrate`
/// (`node-pg-migrate`, `db-migrate`) is not mistaken for a migrations
/// directory.
pub fn is_migration_path(path: &std::path::Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy().to_ascii_lowercase();
        s == "migrations" || s == "migration" || s == "migrate"
    })
}

/// DML statement shapes: a verb and the companion clause keyword that
/// must follow it (in token order) for the string to be a real query.
/// `SELECT … FROM`, `DELETE … FROM`, `INSERT … INTO`, `UPDATE … SET`.
const DML_STATEMENT_SHAPES: &[(&str, &str)] = &[
    ("select", "from"),
    ("delete", "from"),
    ("insert", "into"),
    ("update", "set"),
];

/// SQL keywords that may separate identifiers inside a single projection or
/// target reference (`DISTINCT name`, `id AS user_id`). A span containing one
/// of these is still one projection item, not free-form prose.
const PROJECTION_KEYWORDS: &[&str] = &["distinct", "all", "as"];

/// Returns true if `text` looks like a SQL query. Requires a DML verb
/// followed (in token order) by its mandatory companion clause keyword
/// — `SELECT … FROM`, `DELETE … FROM`, `INSERT … INTO`, `UPDATE … SET`
/// — with the text between them shaped like a SQL projection/target
/// rather than English prose. Word-boundary matching means identifiers
/// containing the keywords don't trigger, the ordering requirement
/// rejects prose that mentions the keywords out of clause structure,
/// and the projection-shape requirement rejects sentences whose words
/// happen to span `select … from`.
pub fn is_sql_string(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    DML_STATEMENT_SHAPES
        .iter()
        .any(|(verb, clause)| dml_shape_matches(&lower, verb, clause))
}

/// Whether `lower` (already lowercase) contains the `verb … clause` DML shape:
/// a whole-word `verb` followed by a whole-word `clause`, with the text between
/// them shaped like a SQL projection/target rather than English prose.
fn dml_shape_matches(lower: &str, verb: &str, clause: &str) -> bool {
    let Some((_, verb_end)) = find_word(lower, verb, 0) else {
        return false;
    };
    let Some((clause_start, _)) = find_word(lower, clause, verb_end) else {
        return false;
    };
    span_is_projection_shaped(&lower[verb_end..clause_start])
}

/// Byte range `(start, end)` of the first whole-word occurrence of `needle`
/// (lowercase) in `haystack` (lowercase) at or after `from`, or `None`. A
/// whole word is not flanked by identifier bytes, so `from` does not match
/// inside `from_user`.
fn find_word(haystack: &str, needle: &str, from: usize) -> Option<(usize, usize)> {
    let bytes = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() || bytes.len() < needle.len() {
        return None;
    }
    let mut i = from;
    while i + needle.len() <= bytes.len() {
        if bytes[i..i + needle.len()] == *needle {
            let before_ok = i == 0 || !is_ident_byte(bytes[i - 1]);
            let after = i + needle.len();
            let after_ok = after >= bytes.len() || !is_ident_byte(bytes[after]);
            if before_ok && after_ok {
                return Some((i, after));
            }
        }
        i += 1;
    }
    None
}

/// Whether the text between a DML verb and its companion clause is shaped like
/// a SQL projection/target rather than English prose.
///
/// A SQL projection separates its parts with punctuation or operators — a
/// column list (`id, name`), qualified name (`a.b`), function call
/// (`count(*)`), expression (`price + tax`) — or wraps a single column in
/// keyword modifiers (`DISTINCT name`, `id AS user_id`). English prose instead
/// runs plain words separated only by whitespace. So the span is prose iff it
/// holds two adjacent non-keyword words with only whitespace between them
/// ("specific fields to fetch"), which is how `SELECT … FROM` matches an
/// English sentence.
///
/// The discriminator is the inter-word separator, so the rare sentence that
/// punctuates between every word (a comma-spliced clause) can still pass, and a
/// bare implicit-alias projection (`SELECT price net FROM orders`, no `AS` and
/// no punctuation) is conversely read as prose. Both are accepted heuristic
/// limits, not a regression of the keyword-order gate.
fn span_is_projection_shaped(span: &str) -> bool {
    let bytes = span.as_bytes();
    let mut prev_was_content_word = false;
    let mut only_whitespace_since_word = true;
    let mut word_start: Option<usize> = None;
    // Iterate one past the end so a trailing word is flushed by the same arm.
    for i in 0..=bytes.len() {
        if i < bytes.len() && is_ident_byte(bytes[i]) {
            word_start.get_or_insert(i);
            continue;
        }
        if let Some(start) = word_start.take() {
            let word = &span[start..i];
            let is_keyword = PROJECTION_KEYWORDS.contains(&word);
            if !is_keyword && prev_was_content_word && only_whitespace_since_word {
                return false;
            }
            prev_was_content_word = !is_keyword;
            only_whitespace_since_word = true;
        }
        if i < bytes.len() && !bytes[i].is_ascii_whitespace() {
            only_whitespace_since_word = false;
        }
    }
    true
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

/// Postgres system-catalog relations that appear in schema-introspection
/// queries. Whole-word matched (via `contains_word`) so user identifiers like
/// `signup_class` don't match `pg_class`.
const SYSTEM_CATALOG_TABLES: &[&str] = &[
    "pg_class",
    "pg_constraint",
    "pg_index",
    "pg_indexes",
    "pg_namespace",
    "pg_attribute",
    "pg_am",
    "pg_type",
    "pg_proc",
    "pg_depend",
    "pg_inherits",
    "pg_enum",
    "pg_description",
    "pg_stat_user_tables",
    "pg_stat_user_indexes",
];

/// True if `text` is a query against a Postgres system catalog
/// (`pg_catalog.*` relation or an `information_schema.*` view).
///
/// System catalogs are tiny, unindexed-for-text metadata tables: the
/// leading-wildcard / function-on-indexed-column performance premises (which
/// assume an index on a large table) do not apply, so SQL-quality rules whose
/// justification is "this defeats an index on a large table" should exempt
/// these queries.
pub fn targets_system_catalog(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if lower.contains("information_schema.") {
        return true;
    }
    SYSTEM_CATALOG_TABLES.iter().any(|t| contains_word(&lower, t))
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
        // SELECT alone, no FROM.
        assert!(!is_sql_string("SELECT 1"));
    }

    #[test]
    fn rejects_clause_without_dml() {
        assert!(!is_sql_string("from start to end where they came from"));
    }

    #[test]
    fn rejects_verb_with_wrong_companion_clause() {
        // issue #3358: "Would update X from Y" pairs `update` with `from`,
        // but UPDATE's companion is SET, not FROM — this is English prose.
        assert!(!is_sql_string(
            "Would update pkg.json from 1.0.0 to 2.0.0 now"
        ));
        // Prisma JSDoc / API surface: `update` + `where` API method names
        // with no SET clause are not a SQL statement.
        assert!(!is_sql_string(
            "Update one user. const u = await prisma.user.update({ where: { id } })"
        ));
        // CLI help text describing a migrate command.
        assert!(!is_sql_string(
            "Update the database schema with migrations"
        ));
    }

    #[test]
    fn rejects_english_prose_spanning_select_from_issue_6903() {
        // "Select specific fields to fetch from the X" — `select … from` appear
        // in clause order, but the span between them ("specific fields to
        // fetch") is free-form prose, not a column projection.
        assert!(!is_sql_string(
            "Select specific fields to fetch from the User"
        ));
        assert!(!is_sql_string("select the value from the cache"));
    }

    #[test]
    fn accepts_single_column_and_keyword_modified_projection() {
        // Single column, qualified name, aliased column, DISTINCT modifier and
        // a `*` projection are all genuine SQL shapes between SELECT and FROM.
        assert!(is_sql_string("SELECT id FROM users"));
        assert!(is_sql_string("SELECT a.b FROM t"));
        assert!(is_sql_string("SELECT id AS user_id FROM users"));
        assert!(is_sql_string("SELECT DISTINCT name FROM users"));
        assert!(is_sql_string("SELECT 1 FROM dual"));
        assert!(is_sql_string("SELECT COUNT(*) FROM users"));
    }

    #[test]
    fn accepts_expression_projection() {
        // An arithmetic expression projection separates its operands with an
        // operator, not whitespace, so it is a SQL shape — not prose. A missed
        // injection here would be the dangerous direction of error.
        assert!(is_sql_string("SELECT a + b FROM t"));
        assert!(is_sql_string("SELECT price - discount FROM orders"));
    }

    #[test]
    fn rejects_hyphenated_english_prose_spanning_select_from() {
        // Hyphenated prose ("read-only fields") must stay rejected: the hyphen
        // separates `read`/`only`, but `only fields` are whitespace-adjacent
        // plain words.
        assert!(!is_sql_string("Select read-only fields from the model"));
    }

    #[test]
    fn rejects_companion_clause_before_verb() {
        // The companion clause must follow the verb in token order.
        assert!(!is_sql_string("from the set of rows, select your favorite"));
    }

    #[test]
    fn detects_delete_from() {
        assert!(is_sql_string("DELETE FROM logs WHERE created_at < now()"));
    }

    #[test]
    fn detects_update_set() {
        assert!(is_sql_string("UPDATE users SET active = false WHERE id = 1"));
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

    #[test]
    fn is_migration_path_matches_migration_dirs() {
        use std::path::Path;
        assert!(is_migration_path(Path::new("myapp/migrations/001.ts")));
        assert!(is_migration_path(Path::new("src/migration/0001.ts")));
        assert!(is_migration_path(Path::new("db/migrate/0001_init.rb")));
        assert!(is_migration_path(Path::new("MIGRATIONS/v1.sql"))); // case-insensitive
    }

    #[test]
    fn is_migration_path_rejects_framework_package_dirs_issue_5793() {
        use std::path::Path;
        // node-pg-migrate / db-migrate-pg own source: the package directory
        // name contains `migrate` as a substring but is not a migrations dir.
        assert!(!is_migration_path(Path::new(
            "node-pg-migrate/src/operations/tables/addConstraint.ts"
        )));
        assert!(!is_migration_path(Path::new("db-migrate/pg/index.js")));
        assert!(!is_migration_path(Path::new(
            "node_modules/db-migrate-pg/index.js"
        )));
    }
}
