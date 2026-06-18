//! sql-singular-table-names

mod rust;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-singular-table-names",
    description: "Table names should be singular nouns.",
    remediation: "Rename `users` -> `user`, `orders` -> `order`. Singular table names match the row-as-entity model and keep joins readable.",
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

/// Extract every plural-looking table name from CREATE TABLE statements
/// inside the SQL string. Each line is examined independently — a single
/// SQL string may contain multiple CREATE TABLEs.
pub(super) fn find_plural_table_names(sql: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in sql.lines() {
        let upper = line.to_ascii_uppercase();
        if !upper.contains("CREATE TABLE") {
            continue;
        }
        if let Some(name) = extract_table_name(&upper, line)
            && looks_plural(&name)
        {
            out.push(name);
        }
    }
    out
}

fn extract_table_name(upper: &str, original: &str) -> Option<String> {
    let idx = upper.find("CREATE TABLE")?;
    let after = &original[idx + "CREATE TABLE".len()..];
    let after_upper = &upper[idx + "CREATE TABLE".len()..];
    let trimmed = after.trim_start();
    let trimmed_upper = after_upper.trim_start();
    let rest = if trimmed_upper.starts_with("IF NOT EXISTS") {
        trimmed["IF NOT EXISTS".len()..].trim_start()
    } else {
        trimmed
    };
    let mut ident = String::new();
    for ch in rest.chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == '.' || ch == '"' {
            ident.push(ch);
        } else {
            break;
        }
    }
    let cleaned = ident.replace('"', "");
    let name = cleaned.rsplit('.').next().unwrap_or(&cleaned).to_string();
    if name.is_empty() { None } else { Some(name) }
}

fn looks_plural(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower.len() < 3 {
        return false;
    }
    const EXCEPTIONS: &[&str] = &["status", "address", "business", "progress", "analysis"];
    if EXCEPTIONS.iter().any(|e| lower == *e) {
        return false;
    }
    lower.ends_with('s') && !lower.ends_with("ss") && !lower.ends_with("us")
}
