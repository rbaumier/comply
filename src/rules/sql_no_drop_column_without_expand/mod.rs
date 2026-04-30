//! sql-no-drop-column-without-expand

mod rust;
mod sql;
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-drop-column-without-expand",
    description: "`DROP COLUMN` without a prior deprecation release breaks running deploys.",
    remediation: "Mark the column unused in a previous release (stop writing/reading it from the app), ship, *then* `DROP COLUMN` in a later migration. Dropping live columns invalidates cached query plans in every connected client.",
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
                Language::JavaScript,
                Backend::TreeSitter(Box::new(typescript::Check)),
            ),
            (
                Language::Tsx,
                Backend::TreeSitter(Box::new(typescript::Check)),
            ),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Sql, Backend::Text(Box::new(sql::Check))),
        ],
    }
}

/// True if the file (anywhere) marks a column as deprecated. Searched in the
/// host source (not the SQL string) because the marker is conventionally
/// written as a code comment near the migration call site.
pub(super) fn file_marks_deprecation(source: &str) -> bool {
    let lower = source.to_ascii_lowercase();
    lower.contains("deprecated in")
        || lower.contains("expand-contract")
        || lower.contains("unused since")
}

/// True if the SQL string drops a column.
pub(super) fn sql_drops_column(sql: &str) -> bool {
    let upper = sql.to_ascii_uppercase();
    upper.contains("DROP COLUMN") && upper.contains("ALTER TABLE")
}
