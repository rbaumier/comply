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

fn extract_constraint_names(sql: &str) -> Vec<String> {
    let upper = sql.to_ascii_uppercase();
    let mut out = Vec::new();
    let mut search_from = 0;
    while let Some(rel) = upper[search_from..].find("CONSTRAINT ") {
        let mut cursor = search_from + rel + "CONSTRAINT ".len();
        let mut name = String::new();
        loop {
            // Skip leading whitespace before the token.
            cursor += sql[cursor..].len() - sql[cursor..].trim_start().len();
            let token: String = sql[cursor..]
                .chars()
                .take_while(|&ch| ch.is_alphanumeric() || ch == '_' || ch == '"')
                .collect();
            if token.is_empty() {
                break;
            }
            cursor += token.len();
            // Skip DDL keywords (IF / NOT / EXISTS) and read the real name next.
            if CONSTRAINT_KEYWORDS.contains(&token.to_ascii_uppercase().as_str()) {
                continue;
            }
            name = token;
            break;
        }
        let cleaned = name.replace('"', "");
        if !cleaned.is_empty() {
            out.push(cleaned);
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
