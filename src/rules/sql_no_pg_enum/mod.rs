//! sql-no-pg-enum

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-pg-enum",
    description: "PostgreSQL `CREATE TYPE ... AS ENUM` — can't remove values once added.",
    remediation: "Replace PG enums with a CHECK constraint (`status TEXT CHECK(status IN ('a','b','c'))`) or a lookup table. PG enums can't have values removed — they're append-only, which makes rollbacks impossible.",
    severity: Severity::Error,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
        ],
    }
}
