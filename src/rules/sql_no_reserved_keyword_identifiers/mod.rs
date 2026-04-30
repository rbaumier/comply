//! sql-no-reserved-keyword-identifiers

mod drizzle;
mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-reserved-keyword-identifiers",
    description: "PostgreSQL reserved words must not be used as table or column names.",
    remediation: "Rename identifiers like `user`, `order`, `group`, `table` — otherwise every reference requires double-quoting, which leaks into ORMs and breaks silently.",
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
        ],
    }
}

const RESERVED: &[&str] = &[
    "USER",
    "ORDER",
    "GROUP",
    "TABLE",
    "SELECT",
    "FROM",
    "WHERE",
    "JOIN",
    "UNION",
    "GRANT",
    "REFERENCES",
    "CHECK",
    "DEFAULT",
    "PRIMARY",
    "FOREIGN",
    "UNIQUE",
    "COLUMN",
    "CONSTRAINT",
    "DESC",
    "ASC",
    "LIMIT",
    "OFFSET",
    "AS",
    "CASE",
    "WHEN",
    "END",
    "RETURNING",
    "VALUES",
];

#[derive(Debug)]
pub(super) enum ReservedHit {
    Table(String),
    Column(String),
}

fn extract_table_name(upper: &str, original: &str) -> Option<String> {
    let idx = upper.find("CREATE TABLE")?;
    let after = &original[idx + "CREATE TABLE".len()..];
    let after_upper = &upper[idx + "CREATE TABLE".len()..];
    let rest = if after_upper.trim_start().starts_with("IF NOT EXISTS") {
        let t = after.trim_start();
        t["IF NOT EXISTS".len()..].trim_start()
    } else {
        after.trim_start()
    };
    let mut ident = String::new();
    for ch in rest.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            ident.push(ch);
        } else {
            break;
        }
    }
    if ident.is_empty() { None } else { Some(ident) }
}

/// Scan the (DDL) SQL string line-by-line for CREATE TABLE / ADD COLUMN with
/// reserved-word identifiers. Multiple hits per string are reported.
pub(super) fn find_reserved_hits(sql: &str) -> Vec<ReservedHit> {
    let mut out = Vec::new();
    for line in sql.lines() {
        let upper = line.to_ascii_uppercase();
        if upper.contains("CREATE TABLE")
            && let Some(name) = extract_table_name(&upper, line)
            && RESERVED.contains(&name.to_ascii_uppercase().as_str())
        {
            out.push(ReservedHit::Table(name));
        }
        if upper.contains("ADD COLUMN ") {
            let pos = upper.find("ADD COLUMN ").unwrap();
            let after = &line[pos + "ADD COLUMN ".len()..].trim_start();
            let mut ident = String::new();
            for ch in after.chars() {
                if ch.is_alphanumeric() || ch == '_' {
                    ident.push(ch);
                } else {
                    break;
                }
            }
            if !ident.is_empty() && RESERVED.contains(&ident.to_ascii_uppercase().as_str()) {
                out.push(ReservedHit::Column(ident));
            }
        }
    }
    out
}
