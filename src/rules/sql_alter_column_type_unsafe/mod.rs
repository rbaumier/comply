//! sql-alter-column-type-unsafe

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-alter-column-type-unsafe",
    description: "`ALTER COLUMN ... TYPE` without a `USING` clause may force a full table rewrite.",
    remediation: "Add a `USING` clause that lets PostgreSQL skip the rewrite when the cast is binary-compatible, or follow the expand/contract pattern: add a new column, backfill, swap, drop the old column.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql", "migrations"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Sql, Backend::Text(Box::new(text::Check)))],
    }
}
