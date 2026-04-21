//! sql-create-index-concurrently

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-create-index-concurrently",
    description: "`CREATE INDEX` without `CONCURRENTLY` takes an `ACCESS EXCLUSIVE` lock, blocking all table access.",
    remediation: "Use `CREATE INDEX CONCURRENTLY` for production migrations. Run outside a transaction block.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql", "migrations"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::Rust, Backend::Text(Box::new(text::Check))),
            (Language::Vue, Backend::Text(Box::new(text::Check))),
        ],
    }
}
