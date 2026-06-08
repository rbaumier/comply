//! sql-create-index-in-transaction

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-create-index-in-transaction",
    description: "`CREATE INDEX CONCURRENTLY` cannot run inside a transaction block.",
    remediation: "Move `CREATE INDEX CONCURRENTLY` outside of `BEGIN`/`COMMIT`. Most migration tools have an option to disable the implicit transaction wrapper for a single migration.",
    severity: Severity::Error,
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
