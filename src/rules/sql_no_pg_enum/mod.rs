//! sql-no-pg-enum

mod oxc_drizzle;
#[cfg(test)]
mod drizzle;
mod rust;
mod text;
mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-pg-enum",
    description: "PostgreSQL `CREATE TYPE ... AS ENUM` — can't remove values once added.",
    remediation: "Replace PG enums with a CHECK constraint (`status TEXT CHECK(status IN ('a','b','c'))`) or a lookup table. PG enums can't have values removed — they're append-only, which makes rollbacks impossible.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database", "sql"],
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
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_drizzle::Check)),
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
            (Language::Vue, Backend::Text(Box::new(text::Check))),
            (Language::Sql, Backend::Text(Box::new(text::Check))),
        ],
    }
}

/// True if `text` contains an `AS ENUM` clause (case-insensitive),
/// which is the marker for `CREATE TYPE ... AS ENUM (...)`.
pub(super) fn declares_pg_enum(text: &str) -> bool {
    text.to_ascii_uppercase().contains("AS ENUM")
}
