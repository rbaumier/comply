//! sql-no-union-when-union-all

mod oxc_drizzle;
#[cfg(test)]
mod drizzle;
mod rust;
mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-union-when-union-all",
    description: "`UNION` forces a dedup sort; prefer `UNION ALL` when rows are already unique.",
    remediation: "If both sides include a primary key or are otherwise guaranteed distinct, use `UNION ALL`. The dedup step in `UNION` requires a hash or sort across the combined set.",
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
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_drizzle::Check)),
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

/// True if the SQL string contains a bare `UNION` (not `UNION ALL`) and the
/// query selects an `id` column from the **same** table on both sides — a
/// proxy for a primary key making the dedup unnecessary.
///
/// When the two sides select from **different** tables the `id` values can
/// overlap, so `UNION ALL` would change the result.
pub(super) fn sql_violates_union_all(sql: &str) -> bool {
    let upper = sql.to_ascii_uppercase();
    let Some(pos) = upper.find("UNION") else {
        return false;
    };
    let after = &upper[pos + "UNION".len()..];
    if after.trim_start().starts_with("ALL") {
        return false;
    }
    let has_id = upper.contains("SELECT ID")
        || upper.contains(" ID,")
        || upper.contains(" ID ")
        || upper.contains(".ID");
    if !has_id {
        return false;
    }
    // Only flag when both sides of the UNION select from the same table.
    let left = &upper[..pos];
    let right = &upper[pos + "UNION".len()..];
    let left_table = extract_from_table(left);
    let right_table = extract_from_table(right);
    match (left_table, right_table) {
        (Some(l), Some(r)) => l == r,
        // Cannot determine tables — don't flag to avoid false positives.
        _ => false,
    }
}

/// Extract the first table name after `FROM` in an SQL fragment.
fn extract_from_table(sql: &str) -> Option<&str> {
    let from_pos = sql.find("FROM")?;
    let after_from = sql[from_pos + "FROM".len()..].trim_start();
    // Take the next whitespace-delimited token as the table name.
    let table = after_from.split_whitespace().next()?;
    // Strip trailing punctuation (parens, commas, semicolons, quotes).
    let table = table.trim_end_matches(|c: char| {
        c == ',' || c == ')' || c == ';' || c == '"' || c == '\'' || c == '`'
    });
    if table.is_empty() { None } else { Some(table) }
}
