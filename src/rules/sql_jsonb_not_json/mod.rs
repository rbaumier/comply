//! sql-jsonb-not-json

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-jsonb-not-json",
    description: "`JSON` stores raw text and re-parses on every read; `JSONB` is the binary, indexable form.",
    remediation: "Use `JSONB` unless you genuinely need to preserve key order or whitespace. `JSONB` supports GIN indexes, path operators, and is faster for every operation except a single insert.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Sql, Backend::Text(Box::new(text::Check)))],
    }
}
