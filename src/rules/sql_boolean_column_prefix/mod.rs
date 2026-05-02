//! sql-boolean-column-prefix

mod drizzle;
mod rust;
mod sql;
mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-boolean-column-prefix",
    description: "BOOLEAN columns should be prefixed with `is_` or `has_`.",
    remediation: "Rename `active BOOLEAN` -> `is_active BOOLEAN`, `admin BOOLEAN` -> `is_admin BOOLEAN`. The prefix makes boolean semantics obvious at call sites.",
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
                Backend::TreeSitter(Box::new(drizzle::Check)),
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
            (Language::Sql, Backend::Text(Box::new(sql::Check))),
        ],
    }
}

/// Scan the (already-confirmed-as-DDL) SQL string for BOOLEAN columns whose
/// name doesn't start with `is_` or `has_`. Returns the offending column
/// names in order. Examines each line independently — column definitions
/// in PostgreSQL DDL are line-oriented in practice.
pub(super) fn find_bad_boolean_columns(sql: &str) -> Vec<String> {
    const KEYWORDS: &[&str] = &[
        "not",
        "null",
        "default",
        "check",
        "unique",
        "constraint",
        "primary",
        "references",
    ];
    let mut out = Vec::new();
    for line in sql.lines() {
        let upper = line.to_ascii_uppercase();
        let kw_pos = upper.find(" BOOLEAN").or_else(|| upper.find(" BOOL "));
        let Some(pos) = kw_pos else { continue };
        let prefix = &line[..pos];
        let Some(col) = prefix
            .rsplit(|c: char| !(c.is_alphanumeric() || c == '_'))
            .find(|tok| !tok.is_empty())
        else {
            continue;
        };
        let lower = col.to_ascii_lowercase();
        if lower.starts_with("is_") || lower.starts_with("has_") {
            continue;
        }
        if KEYWORDS.contains(&lower.as_str()) {
            continue;
        }
        out.push(col.to_string());
    }
    out
}
