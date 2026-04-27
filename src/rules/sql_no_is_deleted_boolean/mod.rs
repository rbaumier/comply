//! sql-no-is-deleted-boolean

mod drizzle;
mod rust;
mod sql;
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-is-deleted-boolean",
    description: "Use `deleted_at TIMESTAMPTZ` instead of `is_deleted BOOLEAN`.",
    remediation: "Soft-delete markers should carry *when* it happened: `deleted_at TIMESTAMPTZ NULL`. A nullable timestamp encodes both the boolean and the event time.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::TypeScript, Backend::TreeSitter(Box::new(drizzle::Check))),
            (Language::JavaScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Tsx, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Sql, Backend::Text(Box::new(sql::Check))),
        ],
    }
}

/// True if the (already-confirmed-as-DDL) SQL string declares an
/// `is_deleted` (or `isDeleted`) column with a BOOLEAN type.
pub(super) fn sql_uses_is_deleted_boolean(sql: &str) -> bool {
    let upper = sql.to_ascii_uppercase();
    let has_col = upper.contains("IS_DELETED") || upper.contains("ISDELETED");
    let has_bool = upper.contains("BOOLEAN") || upper.contains(" BOOL ");
    has_col && has_bool
}
