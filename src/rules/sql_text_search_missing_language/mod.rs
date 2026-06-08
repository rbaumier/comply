//! sql-text-search-missing-language

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-text-search-missing-language",
    description: "`to_tsvector(col)` without an explicit language depends on `default_text_search_config`, which is environment-dependent and not IMMUTABLE.",
    remediation: "Pass the language explicitly: `to_tsvector('english', col)`. The two-argument form is IMMUTABLE and can be used in expression indexes.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql", "indexing"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Sql, Backend::Text(Box::new(text::Check)))],
    }
}
