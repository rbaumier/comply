//! sql-add-not-null-without-default

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-add-not-null-without-default",
    description: "`ALTER COLUMN ... SET NOT NULL` performs a full table scan under an `ACCESS EXCLUSIVE` lock.",
    remediation: "Use the expand/contract pattern: add a `CHECK (col IS NOT NULL) NOT VALID` constraint, then `VALIDATE CONSTRAINT` (which only takes a SHARE UPDATE EXCLUSIVE lock). Drop the check and add `SET NOT NULL` last.",
    severity: Severity::Warning,
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
