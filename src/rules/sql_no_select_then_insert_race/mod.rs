//! sql-no-select-then-insert-race

mod rust;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-select-then-insert-race",
    description: "Sequential SELECT + INSERT on the same key is a TOCTOU race.",
    remediation: "Use `INSERT ... ON CONFLICT (key) DO NOTHING` (or `DO UPDATE`) in a single statement. Two round-trips let concurrent writers insert between the SELECT and INSERT.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database", "sql"],

    // A test file hands its literals to independent calls.
    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::Tsx,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}

/// Reads the table identifier sitting immediately after `keyword` in `upper`
/// (already uppercased SQL). Returns `None` when the target token contains a
/// `{…}` format placeholder (e.g. Rust `format!("… FROM {table_name}")`): the
/// real table is a runtime value, unknown at lint time, and must not be treated
/// as a literal name. Leading whitespace between the keyword and the target is
/// tolerated.
///
/// The name keeps its schema, without quotes: `"app" . "users"` is `APP.USERS`.
/// A Rust literal's escapes go too: `\"app\"` is `APP`.
fn extract_table_after(upper: &str, keyword: &str) -> Option<String> {
    let idx = upper.find(keyword)?;
    let after = upper[idx + keyword.len()..].trim_start();
    let token = after.split_whitespace().next().unwrap_or("");
    if token.contains('{') {
        return None;
    }
    let unquoted = after.replace(['"', '`', '[', ']', '\\'], "");
    let qualified = unquoted
        .split('.')
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(".");
    let is_name_char = |ch: char| ch.is_alphanumeric() || ch == '_';
    let name: String = qualified
        .trim_start_matches(|ch: char| !is_name_char(ch))
        .chars()
        .take_while(|ch| is_name_char(*ch) || *ch == '.')
        .collect();
    let name = name.trim_end_matches('.');
    (!name.is_empty()).then(|| name.to_string())
}

/// The table a read selects from.
/// An `INSERT … SELECT` is one atomic statement, not a read.
pub(super) fn extract_select_from_table(sql: &str) -> Option<String> {
    let upper = sql.to_ascii_uppercase();
    if !upper.contains("SELECT") || upper.contains("INSERT INTO ") {
        return None;
    }
    extract_table_after(&upper, " FROM ")
}

pub(super) fn extract_insert_into_table(sql: &str) -> Option<String> {
    let upper = sql.to_ascii_uppercase();
    extract_table_after(&upper, "INSERT INTO ")
}

pub(super) fn has_on_conflict(sql: &str) -> bool {
    sql.to_ascii_uppercase().contains("ON CONFLICT")
}
