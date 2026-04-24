//! sql-no-uuidv4-primary-key

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-uuidv4-primary-key",
    description: "UUIDv4 primary keys fragment B-tree indexes and bloat storage.",
    remediation: "Use UUIDv7 (time-ordered) for globally-unique keys, or `BIGINT GENERATED ALWAYS AS IDENTITY` for local sequential keys. Avoid `gen_random_uuid()` / `uuid_generate_v4()` on primary keys.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],
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
