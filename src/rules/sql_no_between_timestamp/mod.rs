//! sql-no-between-timestamp — flag `BETWEEN` on a timestamp column.
//!
//! ## Why this rule was rewritten
//!
//! The original implementation was a `TextCheck` that scanned every
//! line of every file looking for the literal substring `between` plus
//! a timestamp-related word (`timestamp`, `created_at`, …). That
//! approach has no notion of "is this actually SQL?" — it would flag
//! comments mentioning the pattern, identifiers like
//! `between_timestamps`, prose docstrings, and any English text that
//! happened to contain both words. The user reported a comment FP and
//! the rule was disabled in the registry while we figured out the
//! right approach.
//!
//! ## How the new rule works
//!
//! The detection is now anchored at string literals in the AST,
//! never at raw bytes:
//!
//! 1. Walk the tree for string-literal nodes — `string` and
//!    `template_string` in TS, `string_literal` and `raw_string_literal`
//!    in Rust.
//! 2. For each string, ask `sql_helpers::is_sql_string` whether it
//!    looks like a real query: at least one DML keyword (`SELECT`,
//!    `INSERT`, `UPDATE`, `DELETE`) AND a clause keyword (`WHERE` or
//!    `FROM`), both whole-word matched.
//! 3. For each SQL string, look for `BETWEEN` whose preceding column
//!    name contains a timestamp hint (`_at`, `time`, `date`,
//!    `timestamp`).
//! 4. Emit the diagnostic at the string literal's start position.
//!
//! ## Language coverage
//!
//! - **TS / JS / TSX**: `typescript` backend, walks `string` and
//!   `template_string` nodes (template literals are how SQL queries
//!   are usually built in TS).
//! - **Rust**: `rust` backend, walks `string_literal` and
//!   `raw_string_literal` nodes (covers both `"…"` and `r#"…"#`).
//! - **Vue**: `vue` backend, extracts each `<script>` block via
//!   `vue_sfc::extract_scripts`, re-parses with the TS grammar, and
//!   runs the same string-walk logic. Diagnostic line/column are
//!   translated back to Vue file coordinates.

mod drizzle;
mod rust;
mod sql_text;
mod typescript;
mod vue;

#[cfg(test)]
mod shared_tests;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-between-timestamp",
    description: "`BETWEEN` with timestamps causes off-by-one bugs (inclusive both sides).",
    remediation: "Replace `BETWEEN start AND end` with `>= start AND < end`. \
                  BETWEEN is inclusive on both sides — midnight rows get \
                  counted twice.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::TreeSitter(Box::new(typescript::Check)),
            ),
            (
                Language::TypeScript,
                Backend::TreeSitter(Box::new(drizzle::Check)),
            ),
            (
                Language::JavaScript,
                Backend::TreeSitter(Box::new(typescript::Check)),
            ),
            (
                Language::Tsx,
                Backend::TreeSitter(Box::new(typescript::Check)),
            ),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Vue, Backend::TreeSitter(Box::new(vue::Check))),
            (Language::Sql, Backend::Text(Box::new(sql_text::Check))),
        ],
    }
}

/// Detect `BETWEEN` clauses whose preceding column name carries a
/// timestamp hint inside an already-confirmed SQL string. Returns
/// true on the first match. Used by every backend so the heuristic
/// stays in one place.
pub(super) fn sql_uses_between_on_timestamp(sql: &str) -> bool {
    let lower = sql.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let needle = b"between";
    let mut search_from = 0usize;
    while let Some(rel) = find_subslice(&bytes[search_from..], needle) {
        let abs = search_from + rel;
        let next = abs + needle.len();
        let before_ok = abs == 0 || !is_ident_byte(bytes[abs - 1]);
        let after_ok = next >= bytes.len() || !is_ident_byte(bytes[next]);
        if before_ok && after_ok {
            // Look at the column name immediately preceding `between`.
            // Walk left over whitespace, then collect identifier bytes
            // (a-z, 0-9, _) until we hit a non-identifier char.
            let column_end = abs.saturating_sub(1);
            let mut i = column_end;
            while i > 0 && bytes[i].is_ascii_whitespace() {
                i -= 1;
            }
            let col_end = i + 1;
            let mut j = col_end;
            while j > 0 && is_ident_byte(bytes[j - 1]) {
                j -= 1;
            }
            let col = &lower[j..col_end];
            if column_is_timestamp(col) {
                return true;
            }
        }
        search_from = next;
    }
    false
}

fn column_is_timestamp(col: &str) -> bool {
    if col.is_empty() {
        return false;
    }
    col.ends_with("_at")
        || col.contains("timestamp")
        || col.contains("time")
        || col.contains("date")
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&i| haystack[i..i + needle.len()] == *needle)
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn flags_between_on_created_at() {
        assert!(sql_uses_between_on_timestamp(
            "SELECT * FROM events WHERE created_at BETWEEN '2024-01-01' AND '2024-12-31'"
        ));
    }

    #[test]
    fn flags_between_on_timestamp_column() {
        assert!(sql_uses_between_on_timestamp(
            "SELECT * FROM logs WHERE event_timestamp BETWEEN ? AND ?"
        ));
    }

    #[test]
    fn does_not_flag_between_on_non_timestamp_column() {
        // BETWEEN on `id` is fine — IDs are integers, not timestamps.
        assert!(!sql_uses_between_on_timestamp(
            "SELECT * FROM users WHERE id BETWEEN 1 AND 100"
        ));
    }

    #[test]
    fn does_not_flag_when_timestamp_is_in_select_only() {
        // `created_at` is in SELECT but BETWEEN is on a different column.
        assert!(!sql_uses_between_on_timestamp(
            "SELECT created_at, name FROM users WHERE id BETWEEN 1 AND 100"
        ));
    }

    #[test]
    fn flags_lowercase() {
        assert!(sql_uses_between_on_timestamp(
            "select * from events where created_at between ? and ?"
        ));
    }
}
