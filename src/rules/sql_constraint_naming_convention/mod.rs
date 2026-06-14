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

fn extract_constraint_names(sql: &str) -> Vec<String> {
    let upper = sql.to_ascii_uppercase();
    let mut out = Vec::new();
    let mut search_from = 0;
    while let Some(rel) = upper[search_from..].find("CONSTRAINT ") {
        let abs = search_from + rel + "CONSTRAINT ".len();
        let after = sql[abs..].trim_start();
        let mut name = String::new();
        for ch in after.chars() {
            if ch.is_alphanumeric() || ch == '_' || ch == '"' {
                name.push(ch);
            } else {
                break;
            }
        }
        let cleaned = name.replace('"', "");
        if !cleaned.is_empty() {
            out.push(cleaned);
        }
        search_from = abs;
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
