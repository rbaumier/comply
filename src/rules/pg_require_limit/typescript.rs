//! pg-require-limit — flag SQL `SELECT` queries that have no `LIMIT`
//! clause and are not already implicitly bounded (aggregate, unique
//! predicate, EXISTS, …).
//!
//! Detection:
//!   Walk `string` and `template_string` nodes. For each one that
//!   looks like SQL (`is_sql_string`), starts with `SELECT`, and does
//!   NOT contain `LIMIT`, check whether it is *implicitly bounded* by
//!   any of:
//!     - aggregate at the top level: `SELECT COUNT(`, `SELECT SUM(`,
//!       `SELECT AVG(`, `SELECT MIN(`, `SELECT MAX(`
//!     - `EXISTS (` — boolean predicate, returns one row
//!     - unique-ish `WHERE ... id = ...` or `WHERE ... id IN (...)`
//!       (heuristic: a whole-word `id` column compared with `=` or `IN`)
//!   If none of those, emit a diagnostic on the string node.
//!
//! We deliberately skip strings that contain dynamic interpolation
//! fragments resembling `${...}` unless the static part still starts
//! with `SELECT` — we don't try to evaluate dynamic queries.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{contains_word, is_sql_string};
use crate::rules::walker::collect_nodes_of_kinds;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();

        for node in collect_nodes_of_kinds(tree, &["string", "template_string"]) {
            let text = extract_string_text(node, source_bytes);
            if text.is_empty() {
                continue;
            }
            if !is_sql_string(&text) {
                continue;
            }
            if !starts_with_select(&text) {
                continue;
            }
            let lower = text.to_ascii_lowercase();
            if contains_word(&lower, "limit") {
                continue;
            }
            if is_implicitly_bounded(&lower) {
                continue;
            }

            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "pg-require-limit".into(),
                message: "SQL `SELECT` without `LIMIT` can return an unbounded number of rows — \
                          add `LIMIT n` or a unique-row predicate (`WHERE id = ...`, `COUNT(..)`)."
                    .into(),
                severity: Severity::Error,
                span: Some((node.byte_range().start, node.byte_range().len())),
            });
        }

        diagnostics
    }
}

/// Extract the logical textual content of a `string` or `template_string`
/// node. For `template_string` we concatenate `string_fragment` children,
/// replacing each `template_substitution` with a space so keyword word-
/// boundary checks still work.
fn extract_string_text(node: tree_sitter::Node<'_>, source: &[u8]) -> String {
    match node.kind() {
        "string" => {
            // The node text includes the surrounding quotes; strip them.
            let raw = node.utf8_text(source).unwrap_or("");
            strip_quotes(raw).to_string()
        }
        "template_string" => {
            let mut cursor = node.walk();
            let mut out = String::new();
            for child in node.named_children(&mut cursor) {
                match child.kind() {
                    "string_fragment" => {
                        if let Ok(t) = child.utf8_text(source) {
                            out.push_str(t);
                        }
                    }
                    "template_substitution" => {
                        out.push(' ');
                    }
                    _ => {}
                }
            }
            out
        }
        _ => String::new(),
    }
}

fn strip_quotes(raw: &str) -> &str {
    let bytes = raw.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' || first == b'\'' || first == b'`') && first == last {
            return &raw[1..raw.len() - 1];
        }
    }
    raw
}

/// True if the first SQL keyword (ignoring leading whitespace / CTE
/// `WITH ... AS (...)` prefix) is `SELECT`. We keep this simple: a
/// leading `SELECT` or `WITH` both qualify — both can be unbounded.
fn starts_with_select(text: &str) -> bool {
    let trimmed = text.trim_start();
    let head: String = trimmed.chars().take(8).collect::<String>().to_ascii_lowercase();
    head.starts_with("select") || head.starts_with("with ") || head.starts_with("with\t")
}

/// Heuristics for "this SELECT is implicitly bounded even without LIMIT".
/// All matching is whole-word, case-insensitive.
fn is_implicitly_bounded(lower: &str) -> bool {
    // Aggregates at the top level — always return one row (per group).
    // We don't have to detect GROUP BY; a query with GROUP BY and no
    // LIMIT is still unbounded if groups are unbounded, so only
    // aggregate-without-group-by should short-circuit. For a
    // conservative heuristic we treat `count(` / `sum(` / `avg(` /
    // `min(` / `max(` as bounded only when there is no `group by`.
    let has_group_by = contains_phrase(lower, "group by");
    if !has_group_by {
        for agg in ["count(", "sum(", "avg(", "min(", "max("] {
            if lower.contains(agg) {
                return true;
            }
        }
    }

    // EXISTS ( ... ) — a boolean subquery; the outer SELECT is returning
    // a single row.
    if lower.contains("exists(") || lower.contains("exists (") {
        return true;
    }

    // Unique-row predicate: `WHERE ... id = ...` or `WHERE ... id in (...)`.
    // We only look for the `id` column (the most common primary key) to
    // stay conservative. `email =`, `uuid =`, etc. are still flagged —
    // the author can either add LIMIT or document a uniqueness predicate.
    if contains_word(lower, "where") && has_id_equality(lower) {
        return true;
    }

    false
}

fn contains_phrase(lower: &str, phrase: &str) -> bool {
    lower.split_whitespace().collect::<Vec<_>>().windows(phrase.split_whitespace().count())
        .any(|window| window.join(" ") == phrase)
}

/// Detects `id = ...` or `id in (...)` as a whole-word `id` column
/// reference followed by an equality/IN operator. Also accepts the
/// qualified form `<alias>.id =`.
fn has_id_equality(lower: &str) -> bool {
    let bytes = lower.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // find `id` as a whole word (possibly preceded by `.` for `u.id`)
        if bytes[i] == b'i'
            && i + 1 < bytes.len()
            && bytes[i + 1] == b'd'
            && (i + 2 == bytes.len() || !is_ident_byte(bytes[i + 2]))
            && (i == 0 || !is_ident_byte(bytes[i - 1]) || bytes[i - 1] == b'.')
        {
            // skip whitespace after `id`
            let mut j = i + 2;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < bytes.len() {
                if bytes[j] == b'=' {
                    return true;
                }
                // `in (` or `in(`
                if j + 1 < bytes.len()
                    && bytes[j] == b'i'
                    && bytes[j + 1] == b'n'
                    && (j + 2 == bytes.len() || !is_ident_byte(bytes[j + 2]))
                {
                    let mut k = j + 2;
                    while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                        k += 1;
                    }
                    if k < bytes.len() && bytes[k] == b'(' {
                        return true;
                    }
                }
            }
        }
        i += 1;
    }
    false
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_select_without_limit_plain_string() {
        let src = r#"const q = "SELECT * FROM users WHERE active = true";"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_select_without_limit_tagged_template() {
        let src = r#"const q = sql`SELECT * FROM users WHERE active = true`;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_select_with_limit() {
        let src = r#"const q = "SELECT * FROM users WHERE active = true LIMIT 10";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_select_with_id_equality() {
        let src = r#"const q = "SELECT * FROM users WHERE id = $1";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_select_with_table_alias_id_equality() {
        let src = r#"const q = "SELECT u.* FROM users u WHERE u.id = $1";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_count_aggregate() {
        let src = r#"const q = "SELECT COUNT(*) FROM users WHERE active = true";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_count_with_group_by_is_still_flagged() {
        // GROUP BY means COUNT per group — can still be unbounded rows.
        let src = r#"const q = "SELECT tenant_id, COUNT(*) FROM users GROUP BY tenant_id";"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn ignores_non_sql_strings() {
        let src = r#"const greeting = "SELECT your plan";"#;
        // No FROM/WHERE clause — is_sql_string returns false.
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_update_statements() {
        // Rule only covers SELECT; UPDATE/DELETE have their own rules.
        let src = r#"const q = "UPDATE users SET active = false WHERE tenant_id = $1";"#;
        assert!(run_on(src).is_empty());
    }
}
