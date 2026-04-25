//! sql-no-disable-autovacuum

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-disable-autovacuum",
    description: "Disabling autovacuum on a table causes bloat and XID wraparound.",
    remediation: "Do not set `autovacuum_enabled = false`. If the default is too aggressive, tune `autovacuum_vacuum_scale_factor` / `autovacuum_vacuum_threshold` instead.",
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

/// True if the SQL string disables autovacuum on a table.
pub(super) fn sql_disables_autovacuum(sql: &str) -> bool {
    let lower = sql.to_ascii_lowercase();
    let compact: String = lower.chars().filter(|c| !c.is_whitespace()).collect();
    compact.contains("autovacuum_enabled=false") || compact.contains("autovacuum_enabled=off")
}
