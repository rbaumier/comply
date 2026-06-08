//! sql-no-uuidv4-primary-key

mod oxc_drizzle;
#[cfg(test)]
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
    id: "sql-no-uuidv4-primary-key",
    description: "UUIDv4 primary keys fragment B-tree indexes and bloat storage.",
    remediation: "Use UUIDv7 (time-ordered) for globally-unique keys, or `BIGINT GENERATED ALWAYS AS IDENTITY` for local sequential keys. Avoid `gen_random_uuid()` / `uuid_generate_v4()` on primary keys.",
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
            (Language::Sql, Backend::Text(Box::new(sql::Check))),
        ],
    }
}

/// Walk the SQL string per-line and return true if a UUIDv4 generator is
/// used on a column declared as a primary key.
pub(super) fn sql_uses_uuidv4_pk(sql: &str) -> bool {
    for line in sql.lines() {
        let upper = line.to_ascii_uppercase();
        let has_v4 = upper.contains("GEN_RANDOM_UUID()") || upper.contains("UUID_GENERATE_V4()");
        if !has_v4 {
            continue;
        }
        let mentions_pk = upper.contains("PRIMARY KEY")
            || upper.contains(" ID UUID")
            || upper.contains("\tID UUID")
            || upper.starts_with("ID UUID");
        if mentions_pk {
            return true;
        }
    }
    false
}
