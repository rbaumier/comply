//! sql-index-needs-rationale-comment — every `CREATE INDEX` must be
//! accompanied by a short SQL `-- ...` rationale explaining which query
//! it accelerates.
//!
//! Indexes are easy to add and terrifying to remove: nobody knows which
//! code path relies on them, so they rot forever, eating write throughput
//! and disk. Requiring a one-line rationale at the point of creation
//! gives future readers enough context to prune dead indexes.
//!
//! Cross-language: raw SQL is embedded in TS/TSX/JS template literals
//! (drizzle, sqlx-style wrappers, knex) and in Rust string / raw-string
//! literals (sqlx, diesel, tokio-postgres). Both backends walk string
//! nodes in the AST and scan their contents. Pure `.sql` migration
//! files run the same scan against the whole file content.

mod rust;
mod sql_text;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-index-needs-rationale-comment",
    description: "`CREATE INDEX` without a SQL comment explaining why the index exists.",
    remediation: "Prefix the `CREATE INDEX` with an SQL comment (`-- Accelerates the \
                  dashboard timeline query filtered by user_id, ordered by created_at DESC`) \
                  explaining which query the index supports. Without this note, nobody \
                  dares drop the index even when the underlying query is gone.",
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
            (Language::Sql, Backend::Text(Box::new(sql_text::Check))),
        ],
    }
}
