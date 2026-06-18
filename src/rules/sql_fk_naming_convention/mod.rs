//! sql-fk-naming-convention

mod rust;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-fk-naming-convention",
    description: "Foreign keys must be named `{from_table}_{from_col}_{to_table}_{to_col}_fk`.",
    remediation: "Use a full FK name like `order_user_id_user_id_fk` — it makes both sides of the join visible in error messages and migration logs.",
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

#[derive(Debug)]
pub(super) enum FkViolation {
    MissingConstraintClause,
    BadShape(String),
}

fn extract_constraint_name(line: &str) -> Option<String> {
    let upper = line.to_ascii_uppercase();
    let idx = upper.find("CONSTRAINT ")?;
    let after = &line[idx + "CONSTRAINT ".len()..].trim_start();
    let mut name = String::new();
    for ch in after.chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == '"' {
            name.push(ch);
        } else {
            break;
        }
    }
    let cleaned = name.replace('"', "");
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// Scan the (already-confirmed-as-DDL) SQL string line by line for FOREIGN KEY
/// declarations and return any naming violations. Line-oriented so each FK in a
/// multi-FK statement is reported separately.
pub(super) fn find_fk_violations(sql: &str) -> Vec<FkViolation> {
    let mut out = Vec::new();
    for line in sql.lines() {
        let upper = line.to_ascii_uppercase();
        if !upper.contains("FOREIGN KEY") {
            continue;
        }
        let Some(name) = extract_constraint_name(line) else {
            out.push(FkViolation::MissingConstraintClause);
            continue;
        };
        let lower = name.to_ascii_lowercase();
        let segments: Vec<&str> = lower.split('_').collect();
        let ends_fk = lower.ends_with("_fk");
        let shape_ok = ends_fk && segments.len() >= 5;
        if !shape_ok {
            out.push(FkViolation::BadShape(name));
        }
    }
    out
}
