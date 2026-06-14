//! sql-constraint-naming-convention

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
    id: "sql-constraint-naming-convention",
    description: "Constraints must follow `{table}_{col}_{suffix}` where suffix is pk|fk|key|chk|exl|idx|pkey|fkey.",
    remediation: "Name constraints explicitly: `CONSTRAINT user_email_key UNIQUE (email)`, `CONSTRAINT order_user_id_fk FOREIGN KEY ...`. Deterministic names simplify migrations and error messages.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],

    skip_in_test_dir: false,
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

const VALID_SUFFIXES: &[&str] = &["pk", "fk", "key", "chk", "exl", "idx", "pkey", "fkey"];

/// SQL keywords that may legally follow `CONSTRAINT` before the constraint
/// identifier (e.g. `DROP CONSTRAINT IF EXISTS name`), and must be skipped.
const CONSTRAINT_KEYWORDS: &[&str] = &["IF", "NOT", "EXISTS"];

/// Reads the constraint identifier starting at `cursor`, skipping leading
/// whitespace and any leading DDL keywords (`IF` / `NOT` / `EXISTS`). Returns
/// the raw token (quotes kept) and the cursor positioned right after it.
fn read_name_token(sql: &str, mut cursor: usize) -> (String, usize) {
    loop {
        // Skip leading whitespace before the token.
        cursor += sql[cursor..].len() - sql[cursor..].trim_start().len();
        let token: String = sql[cursor..]
            .chars()
            .take_while(|&ch| ch.is_alphanumeric() || ch == '_' || ch == '"')
            .collect();
        if token.is_empty() {
            return (String::new(), cursor);
        }
        cursor += token.len();
        // Skip DDL keywords (IF / NOT / EXISTS) and read the real name next.
        if CONSTRAINT_KEYWORDS.contains(&token.to_ascii_uppercase().as_str()) {
            continue;
        }
        return (token, cursor);
    }
}

/// True when the `CONSTRAINT` keyword at `kw_start` is part of a
/// `RENAME CONSTRAINT old TO new` clause, where the first name is being removed.
fn is_rename_constraint(upper: &str, kw_start: usize) -> bool {
    upper[..kw_start]
        .trim_end()
        .rsplit(|c: char| c.is_whitespace())
        .next()
        == Some("RENAME")
}

fn extract_constraint_names(sql: &str) -> Vec<String> {
    let upper = sql.to_ascii_uppercase();
    let mut out = Vec::new();
    let mut search_from = 0;
    while let Some(rel) = upper[search_from..].find("CONSTRAINT ") {
        let kw_start = search_from + rel;
        let (name, mut cursor) = read_name_token(sql, kw_start + "CONSTRAINT ".len());

        if is_rename_constraint(&upper, kw_start) {
            // `RENAME CONSTRAINT old TO new`: the first name (`old`) is being
            // removed, so it is not validated; only the new name after `TO` is.
            if let Some(rel_to) = upper[cursor..].find(" TO ") {
                let after_to = cursor + rel_to + " TO ".len();
                let (new_name, new_cursor) = read_name_token(sql, after_to);
                cursor = new_cursor;
                let cleaned = new_name.replace('"', "");
                if !cleaned.is_empty() {
                    out.push(cleaned);
                }
            }
        } else {
            let cleaned = name.replace('"', "");
            if !cleaned.is_empty() {
                out.push(cleaned);
            }
        }
        // Resume strictly after the consumed region so a name ending in
        // `constraint` (e.g. `dim_size_constraint`) is not re-matched.
        search_from = cursor;
    }
    out
}

fn has_valid_suffix(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    VALID_SUFFIXES
        .iter()
        .any(|s| lower.ends_with(&format!("_{s}")))
}

/// Returns names of constraints in the SQL string that don't end in a valid suffix.
pub(super) fn find_bad_constraint_names(sql: &str) -> Vec<String> {
    extract_constraint_names(sql)
        .into_iter()
        .filter(|n| !has_valid_suffix(n))
        .collect()
}
