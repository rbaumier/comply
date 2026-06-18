//! db-no-string-concat-sql — Rust backend.
//!
//! Detects `format!("SELECT ... {}", var)` style SQL injection. The
//! detection is anchored at the *format string* (first string literal
//! inside the macro's `token_tree`), never at the macro's full text.
//! Identifiers in the macro arguments are ignored, so
//! `format!("…: {}", String::from_utf8_lossy(stderr))` no longer
//! gets flagged just because `from_utf8_lossy` contains `from`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::is_sql_string;

use super::position::placeholder_is_identifier_position;

const FORMAT_MACROS: &[&str] = &[
    "format",
    "format_args",
    "write",
    "writeln",
    "print",
    "println",
    "eprint",
    "eprintln",
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["macro_invocation"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(mac) = node.child_by_field_name("macro") else {
            return;
        };
        let Ok(mac_name) = mac.utf8_text(source_bytes) else {
            return;
        };
        if !FORMAT_MACROS.contains(&mac_name) {
            return;
        }
        let Some(format_string) = first_string_literal_in_macro(node, source_bytes) else {
            return;
        };
        if !is_sql_string(format_string) {
            return;
        }
        // Require a risky interpolation — a runtime, non-const value
        // substituted into the SQL. A bare `format!("SELECT ...")`
        // (no placeholders), escaped `{{`/`}}` literal braces (e.g.
        // ClickHouse `{{db:String}}` parameter syntax), and inline
        // compile-time const placeholders (`{COLUMN_KIND}`) are all
        // safe and must not be flagged.
        if !has_risky_placeholder(format_string) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "db-no-string-concat-sql".into(),
            message: "String interpolation with SQL keywords — use \
                      parameterized queries (`$1`, `?`) instead."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// Walk the macro invocation's children for the first string literal.
/// `format!("…", x, y)` exposes its arguments inside a `token_tree`
/// child. The first `string_literal` / `raw_string_literal` we find
/// is the format string.
fn first_string_literal_in_macro<'src>(
    node: tree_sitter::Node,
    source: &'src [u8],
) -> Option<&'src str> {
    let mut cursor = node.walk();
    let mut stack: Vec<tree_sitter::Node> = node.children(&mut cursor).collect();
    while let Some(child) = stack.pop() {
        if matches!(child.kind(), "string_literal" | "raw_string_literal") {
            // Strip the leading/trailing quote bytes for both `"…"` and
            // `r#"…"#` forms — `is_sql_string` doesn't care about the
            // delimiters, but stripping them keeps the search space
            // tight.
            return child.utf8_text(source).ok();
        }
        let mut sub = child.walk();
        for grand in child.children(&mut sub) {
            stack.push(grand);
        }
    }
    None
}

/// Whether the format string contains at least one placeholder that
/// interpolates a runtime, non-const value.
///
/// Applies Rust's format-string rules to the literal text:
/// - `{{` and `}}` are escaped literal braces (consumed as a pair),
///   not placeholders — ClickHouse `{{p:String}}` parameter syntax
///   yields a literal `{p:String}` and contributes no interpolation.
/// - A real `{...}` placeholder is risky unless its argument name is a
///   SCREAMING_SNAKE_CASE identifier, which denotes an inline
///   compile-time const (`{COLUMN_KIND}`), not user input. A positional
///   `{}` / `{0}` is filled by a macro argument expression and is risky.
/// - A placeholder in a SQL identifier position (`FROM {t}`, `UPDATE {t}`,
///   `alias.{col}`, …) names a relation/column. Identifiers cannot be bind
///   parameters (`SELECT * FROM $1` is a parse error), so interpolating one
///   is the only possible form and is not risky.
fn has_risky_placeholder(format_string: &str) -> bool {
    let bytes = format_string.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' if bytes.get(i + 1) == Some(&b'{') => i += 2,
            b'}' if bytes.get(i + 1) == Some(&b'}') => i += 2,
            b'{' => {
                let Some(end) = format_string[i + 1..].find('}') else {
                    return false;
                };
                let inner = &format_string[i + 1..i + 1 + end];
                let name = inner.split(':').next().unwrap_or("");
                if !is_const_name(name) && !placeholder_is_identifier_position(format_string, i) {
                    return true;
                }
                i += 1 + end + 1;
            }
            _ => i += 1,
        }
    }
    false
}

/// Whether `name` is a SCREAMING_SNAKE_CASE identifier — a compile-time
/// const captured inline by `format!` (safe), e.g. `COLUMN_KIND_REGULAR`.
///
/// Requires at least one letter, all letters uppercase, and only
/// `A-Z` / `0-9` / `_`. A positional placeholder (empty `name`) or an
/// index (`0`) has no letter and is therefore not a const.
fn is_const_name(name: &str) -> bool {
    let mut has_letter = false;
    for ch in name.chars() {
        match ch {
            'A'..='Z' => has_letter = true,
            '0'..='9' | '_' => {}
            _ => return false,
        }
    }
    has_letter
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_format_with_sql_select() {
        let src = r#"fn f(id: i32) { let q = format!("SELECT * FROM users WHERE id = {}", id); }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_format_with_sql_update() {
        let src = r#"fn f(id: i32) { let q = format!("UPDATE users SET name = '{}' WHERE id = 1", name); }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn does_not_flag_format_with_from_utf8_lossy_arg() {
        // The exact FP from the user's report. The format string is
        // not SQL; the arg expression contains `from_utf8_lossy`,
        // which used to fool the substring scan.
        let src = r#"fn f(stderr: &[u8]) -> String { format!("failed to parse oxlint JSON output. oxlint stderr: {}", String::from_utf8_lossy(stderr)) }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_static_sql_without_interpolation() {
        let src = r#"fn f() { let q = format!("SELECT * FROM users WHERE id = 1"); }"#;
        // No `{}` interpolation — caller could have written a string literal.
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_non_sql_format() {
        let src = r#"fn f(x: i32) { let s = format!("hello {}", x); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_non_format_macro() {
        let src = r#"fn f() { vec!["SELECT * FROM users WHERE id = {}", "x"]; }"#;
        // `vec!` isn't a format macro; not our concern.
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_clickhouse_parameterized_query() {
        // Issue #1442: escaped `{{db:String}}` braces are ClickHouse
        // parameters (literal `{db:String}` in the output, bound
        // separately) and the `{COLUMN_KIND_*}` are inline consts.
        let src = r#"fn f() {
            let query = format!(
                "SELECT name, type, default_kind \
                 FROM system.columns \
                 WHERE database = {{db:String}} AND table = {{tbl:String}} \
                 AND default_kind IN ('{COLUMN_KIND_REGULAR}', '{COLUMN_KIND_DEFAULT}') \
                 ORDER BY position FORMAT JSONEachRow"
            );
        }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_only_escaped_braces() {
        let src = r#"fn f() { let q = format!("SELECT * FROM t WHERE x = {{p:String}}"); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_only_const_placeholder() {
        let src = r#"fn f() { let q = format!("SELECT * FROM t WHERE k = '{COLUMN_KIND}'"); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_positional_runtime_arg() {
        let src = r#"fn f(user_id: i32) { let q = format!("SELECT * FROM t WHERE id = {}", user_id); }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_inline_lowercase_runtime_var() {
        let src = r#"fn f() { let q = format!("SELECT * FROM t WHERE name = '{user_name}'"); }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    // Issue #3878 — a placeholder in an identifier position names a relation or
    // column. Identifiers cannot be bind parameters, so interpolating one is the
    // only possible form, not an injection. The sqlx repro already parameterizes
    // the value (`$1` + `.bind`) and only interpolates the table identifier.
    #[test]
    fn does_not_flag_table_name_after_delete_from() {
        let src = r##"fn f(table_name: &str) { let q = format!(r#"DELETE FROM {table_name} WHERE version = $1"#); }"##;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_table_name_after_select_from() {
        let src = r#"fn f(t: &str) { let q = format!("SELECT * FROM {t}"); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_table_name_after_update() {
        let src = r#"fn f(t: &str) { let q = format!("UPDATE {t} SET x = $1"); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_table_name_after_insert_into() {
        let src = r#"fn f(t: &str) { let q = format!("INSERT INTO {t} (a) VALUES ($1)"); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_table_name_after_join() {
        let src = r#"fn f(t: &str) { let q = format!("SELECT * FROM a JOIN {t} ON a.id = b.id"); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_dot_qualified_column() {
        let src = r#"fn f(pid_type: &str) { let q = format!("SELECT pg_stat_activity.{pid_type} FROM pg_stat_activity"); }"#;
        assert!(run_on(src).is_empty());
    }

    // #3878 guard — a value-position placeholder is still a real injection and
    // must keep firing even though the same string also interpolates a table
    // identifier (the value, not the identifier, is the risk).
    #[test]
    fn flags_value_placeholder_alongside_identifier_placeholder() {
        let src = r#"fn f(t: &str, user_id: i32) { let q = format!("SELECT * FROM {t} WHERE id = {user_id}"); }"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
