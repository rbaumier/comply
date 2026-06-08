//! sql-drop-table-no-cascade-warning

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-drop-table-no-cascade-warning",
    description: "`DROP TABLE` without `IF EXISTS` fails noisily on rerun, and `DROP TABLE ... CASCADE` silently destroys dependent objects.",
    remediation: "Add `IF EXISTS` so reruns are idempotent. Avoid `CASCADE` — drop dependents explicitly so the migration documents what gets removed.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql", "migrations"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Sql, Backend::Text(Box::new(text::Check)))],
    }
}
