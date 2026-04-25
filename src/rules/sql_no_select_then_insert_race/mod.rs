//! sql-no-select-then-insert-race

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-select-then-insert-race",
    description: "Sequential SELECT + INSERT on the same key is a TOCTOU race.",
    remediation: "Use `INSERT ... ON CONFLICT (key) DO NOTHING` (or `DO UPDATE`) in a single statement. Two round-trips let concurrent writers insert between the SELECT and INSERT.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Tsx, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}

pub(super) fn extract_select_from_table(sql: &str) -> Option<String> {
    let upper = sql.to_ascii_uppercase();
    if !upper.contains("SELECT") {
        return None;
    }
    let idx = upper.find(" FROM ")?;
    let after = &upper[idx + " FROM ".len()..];
    let mut name = String::new();
    for ch in after.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            name.push(ch);
        } else if name.is_empty() {
            continue;
        } else {
            break;
        }
    }
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

pub(super) fn extract_insert_into_table(sql: &str) -> Option<String> {
    let upper = sql.to_ascii_uppercase();
    let idx = upper.find("INSERT INTO ")?;
    let after = &upper[idx + "INSERT INTO ".len()..];
    let mut name = String::new();
    for ch in after.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            name.push(ch);
        } else if name.is_empty() {
            continue;
        } else {
            break;
        }
    }
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

pub(super) fn has_on_conflict(sql: &str) -> bool {
    sql.to_ascii_uppercase().contains("ON CONFLICT")
}
