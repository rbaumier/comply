//! sql-no-truncate-in-app

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-truncate-in-app",
    description: "`TRUNCATE` bypasses triggers, FK checks, and row-level audit.",
    remediation: "Use `DELETE FROM table` so triggers, FK cascades and audit logs fire. `TRUNCATE` belongs to ops-only maintenance scripts, not application queries.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Tsx, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}

/// True if the string contains the SQL `TRUNCATE` statement (whole word).
pub(super) fn sql_uses_truncate(sql: &str) -> bool {
    let lower = sql.to_ascii_lowercase();
    crate::rules::sql_helpers::contains_word(&lower, "truncate")
}
