//! sql-check-constraint-no-volatile-function

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-check-constraint-no-volatile-function",
    description: "`CHECK` constraints with volatile functions (`NOW()`, `random()`, …) violate the relational model — the constraint may pass on insert but fail on dump/restore.",
    remediation: "Move time-based validation into a trigger or application code. CHECK constraints must be deterministic: PostgreSQL is allowed to assume they always evaluate to the same answer for the same row.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database", "sql", "constraints"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Sql, Backend::Text(Box::new(text::Check)))],
    }
}
