//! Position classification for interpolated placeholders in a SQL string.
//!
//! SQL bind parameters (`$1`, `?`) are only legal in *value* positions; in an
//! *identifier* position (a table or column name) they are a hard parse error
//! (`SELECT * FROM $1` → `syntax error at or near "$1"`). So a placeholder that
//! names a relation or column cannot be a bind parameter, and interpolating it
//! is the only possible form — not an injection the rule should flag.

/// Keywords whose following token is a SQL identifier (relation or, for
/// `UPDATE`/`TABLE`, the target table). A placeholder right after one of these
/// names a relation/column and cannot be parameterized.
const IDENTIFIER_KEYWORDS: &[&str] = &["FROM", "JOIN", "INTO", "UPDATE", "TABLE"];

/// Whether the placeholder beginning at `brace_index` in `sql_text` sits in an
/// identifier position (a table/column name) rather than a value position.
///
/// Identifier position holds when the text before the placeholder, ignoring
/// trailing whitespace, either ends with `.` (a `.`-qualified member such as
/// `alias.{col}` or `schema.{table}`) or has, as its last word token, one of
/// [`IDENTIFIER_KEYWORDS`] (case-insensitive). Everything else — after `=`,
/// inside quotes, inside a `VALUES (…)` list, at string start — is a value
/// position, where a placeholder is a real injection vector.
///
/// `brace_index` is the byte offset of the placeholder's opening delimiter
/// (`{` for Rust format strings, `$` of `${` for JS/TS template literals); only
/// the preceding text is inspected, so the delimiter style does not matter.
pub(super) fn placeholder_is_identifier_position(sql_text: &str, brace_index: usize) -> bool {
    let before = sql_text[..brace_index].trim_end_matches([' ', '\t', '\r', '\n']);
    if before.ends_with('.') {
        return true;
    }
    let last_word: String = before
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    IDENTIFIER_KEYWORDS
        .iter()
        .any(|kw| last_word.eq_ignore_ascii_case(kw))
}

/// Whether *every* interpolation point in a multi-fragment SQL string sits in an
/// identifier position — i.e. none of them is a value-position injection vector.
///
/// `fragments` are the `n + 1` static text pieces around `n` interpolation
/// points, in source order (a template literal's quasis, or the two string
/// operands of a `+` concat). Interpolation `i` is preceded by the
/// concatenation of `fragments[0..=i]`; the trailing `fragments[n]` follows the
/// last interpolation and has none after it. Returns `false` when there is no
/// interpolation point (`fragments.len() < 2`), since the caller only invokes
/// this once interpolation is established.
pub(super) fn all_substitutions_in_identifier_position(fragments: &[&str]) -> bool {
    if fragments.len() < 2 {
        return false;
    }
    let mut prefix = String::new();
    // The last fragment trails the final interpolation, so iterate over every
    // fragment except it: each one ends at an interpolation point.
    for fragment in &fragments[..fragments.len() - 1] {
        prefix.push_str(fragment);
        if !placeholder_is_identifier_position(&prefix, prefix.len()) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Locate the byte index of the first `{` in a Rust-style format string.
    fn brace(sql: &str) -> usize {
        sql.find('{').expect("test input must contain a placeholder")
    }

    fn is_ident_pos(sql: &str) -> bool {
        placeholder_is_identifier_position(sql, brace(sql))
    }

    #[test]
    fn from_clause_is_identifier_position() {
        assert!(is_ident_pos("DELETE FROM {table_name} WHERE version = 1"));
        assert!(is_ident_pos("SELECT * FROM {t}"));
    }

    #[test]
    fn update_into_join_table_are_identifier_position() {
        assert!(is_ident_pos("UPDATE {t} SET x = 1"));
        assert!(is_ident_pos("INSERT INTO {t} (a) VALUES (1)"));
        assert!(is_ident_pos("SELECT * FROM a JOIN {t} ON a.id = t.id"));
        assert!(is_ident_pos("CREATE TABLE {t} (id int)"));
    }

    #[test]
    fn keyword_match_is_case_insensitive() {
        assert!(is_ident_pos("select * from {t}"));
        assert!(is_ident_pos("delete From {t}"));
    }

    #[test]
    fn dot_qualified_member_is_identifier_position() {
        assert!(is_ident_pos("SELECT pg_stat_activity.{pid_type} FROM pg_stat_activity"));
        assert!(is_ident_pos("SELECT a.{col} FROM a"));
    }

    #[test]
    fn newline_before_placeholder_still_identifier_position() {
        assert!(is_ident_pos("DELETE FROM\n    {table_name}\n    WHERE version = 1"));
    }

    #[test]
    fn value_position_is_not_identifier_position() {
        assert!(!is_ident_pos("SELECT * FROM t WHERE id = {user_id}"));
        assert!(!is_ident_pos("INSERT INTO t (a) VALUES ({val})"));
        assert!(!is_ident_pos("UPDATE t SET name = '{user_name}'"));
    }

    #[test]
    fn placeholder_at_string_start_is_value_position() {
        assert!(!placeholder_is_identifier_position("{x} FROM t", 0));
    }

    #[test]
    fn keyword_substring_does_not_match() {
        // `wherefrom` ends in `from` as a substring but the whole word token is
        // `wherefrom`, not `from`, so it is a value position.
        assert!(!is_ident_pos("SELECT wherefrom{x}"));
    }

    #[test]
    fn all_substitutions_identifier_when_every_point_is_identifier() {
        // `SELECT * FROM ${t}` — sole interpolation after FROM.
        assert!(all_substitutions_in_identifier_position(&[
            "SELECT * FROM ",
            ""
        ]));
        // `SELECT * FROM ${a} JOIN ${b} ON 1 = 1` — both after FROM/JOIN.
        assert!(all_substitutions_in_identifier_position(&[
            "SELECT * FROM ",
            " JOIN ",
            " ON 1 = 1"
        ]));
    }

    #[test]
    fn all_substitutions_false_when_any_point_is_value() {
        // `SELECT * FROM ${t} WHERE id = ${id}` — second point is a value.
        assert!(!all_substitutions_in_identifier_position(&[
            "SELECT * FROM ",
            " WHERE id = ",
            ""
        ]));
        // Single value-position interpolation.
        assert!(!all_substitutions_in_identifier_position(&[
            "SELECT * FROM t WHERE id = ",
            ""
        ]));
    }

    #[test]
    fn no_interpolation_is_not_all_identifier() {
        assert!(!all_substitutions_in_identifier_position(&["SELECT * FROM t"]));
        assert!(!all_substitutions_in_identifier_position(&[]));
    }
}
