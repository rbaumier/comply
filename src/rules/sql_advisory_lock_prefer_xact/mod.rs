//! sql-advisory-lock-prefer-xact

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
    id: "sql-advisory-lock-prefer-xact",
    description: "`pg_advisory_lock` holds until session ends, leaking if the connection is reused. Use `pg_advisory_xact_lock` instead.",
    remediation: "Replace `pg_advisory_lock(key)` with `pg_advisory_xact_lock(key)` — it auto-releases at transaction end.",
    severity: Severity::Warning,
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

/// True if `text` calls `pg_advisory_lock(` (the session-scoped variant)
/// without using the transaction-scoped or try variants.
pub(super) fn uses_session_advisory_lock(text: &str) -> bool {
    if !text.contains("pg_advisory_lock(") {
        return false;
    }
    if text.contains("pg_advisory_xact_lock(") || text.contains("pg_try_advisory") {
        return false;
    }
    true
}
