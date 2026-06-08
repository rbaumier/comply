//! sql-pg-enum-with-alter-type-add-value

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-pg-enum-with-alter-type-add-value",
    description: "`ALTER TYPE ... ADD VALUE` cannot run inside a transaction block before PostgreSQL 12, and even on newer versions the new value is not usable in the same transaction.",
    remediation: "Run `ALTER TYPE ... ADD VALUE` outside `BEGIN`/`COMMIT`. If you need the new value within a transaction, prefer a CHECK-constrained text column over a true ENUM.",
    severity: Severity::Error,
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
