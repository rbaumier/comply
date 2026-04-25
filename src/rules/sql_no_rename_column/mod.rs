//! sql-no-rename-column

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-rename-column",
    description: "`ALTER TABLE ... RENAME COLUMN` breaks running deploys.",
    remediation: "Use expand-contract: add the new column, dual-write from the app, backfill, switch reads, drop the old column in a later release. A single RENAME COLUMN is a breaking change for any process with cached query plans.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Tsx, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}

pub(super) fn sql_renames_column(sql: &str) -> bool {
    let upper = sql.to_ascii_uppercase();
    upper.contains("RENAME COLUMN") && upper.contains("ALTER TABLE")
}
