//! sql-no-function-on-indexed-column

mod oxc_drizzle;
#[cfg(test)]
mod drizzle;
mod rust;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-function-on-indexed-column",
    description: "Wrapping a column in a function inside WHERE kills index sargability.",
    remediation: "Avoid `WHERE date_trunc('day', created_at) = ...` / `WHERE LOWER(email) = ...`. Store the normalized form, or add a functional index.",
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

const BAD_FUNCS: &[&str] = &[
    "DATE_TRUNC(",
    "LOWER(",
    "UPPER(",
    "COALESCE(",
    "EXTRACT(",
    "CAST(",
    "TO_CHAR(",
];

/// If the SQL string contains a banned function call after a WHERE clause,
/// return the function name (without trailing paren). Otherwise None.
pub(super) fn find_bad_func_in_where(sql: &str) -> Option<&'static str> {
    let upper = sql.to_ascii_uppercase();
    let where_pos = upper.find("WHERE")?;
    let after = &upper[where_pos..];
    for func in BAD_FUNCS {
        if after.contains(func) {
            return Some(func.trim_end_matches('('));
        }
    }
    None
}
