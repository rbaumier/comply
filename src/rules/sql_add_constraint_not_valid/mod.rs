//! sql-add-constraint-not-valid

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
    id: "sql-add-constraint-not-valid",
    description: "ALTER TABLE ADD CONSTRAINT must use NOT VALID then a separate VALIDATE.",
    remediation: "Split the migration: first `ALTER TABLE t ADD CONSTRAINT ... NOT VALID`, then in a later step `ALTER TABLE t VALIDATE CONSTRAINT ...`. Otherwise the ADD takes an AccessExclusiveLock while scanning the whole table.",
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
            (Language::Sql, Backend::Text(Box::new(sql::Check))),
        ],
    }
}

/// True if the (already-confirmed-as-DDL) SQL string contains `ADD CONSTRAINT`
/// for a CHECK or FOREIGN KEY without `NOT VALID`. These are the scan-heavy
/// constraints that require an AccessExclusiveLock for the duration of a
/// table scan when added without `NOT VALID`.
pub(super) fn sql_violates_add_constraint(sql: &str) -> bool {
    let upper = sql.to_ascii_uppercase();
    if !upper.contains("ADD CONSTRAINT") {
        return false;
    }
    let is_scan_heavy = upper.contains("CHECK") || upper.contains("FOREIGN KEY");
    if !is_scan_heavy {
        return false;
    }
    !upper.contains("NOT VALID")
}
