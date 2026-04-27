//! drizzle-migrations-no-data-in-schema-migration

mod sql_text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-migrations-no-data-in-schema-migration",
    description: "A migration mixes DDL (CREATE / ALTER / DROP TABLE) with DML (INSERT / UPDATE / DELETE) — the two should ship in separate migrations.",
    remediation: "Split the schema change and the data change into two `drizzle-kit generate` migrations so each can be reviewed and rolled back independently.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle", "database"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Sql, Backend::Text(Box::new(sql_text::Check)))],
    }
}
