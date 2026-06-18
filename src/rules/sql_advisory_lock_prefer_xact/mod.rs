//! sql-advisory-lock-prefer-xact

mod rust;
mod text;
mod oxc_typescript;

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

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
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
    // A session-level lock is the ONLY variant that can serialize a statement
    // which cannot run inside a transaction block (CREATE/DROP DATABASE,
    // CREATE/DROP TABLESPACE): an `xact` lock would already be released before
    // that statement runs, so don't suggest it here.
    if spans_non_transactional_statement(text) {
        return false;
    }
    true
}

/// True if `text` contains a PostgreSQL statement that cannot run inside a
/// transaction block (and so cannot be covered by a transaction-level lock).
pub(super) fn spans_non_transactional_statement(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let normalized: String = lower.split_whitespace().collect::<Vec<_>>().join(" ");
    [
        "create database",
        "drop database",
        "create tablespace",
        "drop tablespace",
    ]
    .iter()
    .any(|kw| normalized.contains(kw))
}
