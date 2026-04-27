//! sql-index-on-low-cardinality-boolean

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-index-on-low-cardinality-boolean",
    description: "A B-tree index on a boolean column rarely helps — selectivity is too low for the planner to pick it.",
    remediation: "Use a partial index (`CREATE INDEX ... WHERE flag = TRUE`) targeting the rarer value, or drop the index entirely. Plain B-tree on `BOOLEAN` is almost never used.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql", "indexing"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Sql, Backend::Text(Box::new(text::Check)))],
    }
}
