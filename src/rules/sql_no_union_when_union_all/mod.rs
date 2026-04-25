//! sql-no-union-when-union-all

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-union-when-union-all",
    description: "`UNION` forces a dedup sort; prefer `UNION ALL` when rows are already unique.",
    remediation: "If both sides include a primary key or are otherwise guaranteed distinct, use `UNION ALL`. The dedup step in `UNION` requires a hash or sort across the combined set.",
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

/// True if the SQL string contains a bare `UNION` (not `UNION ALL`) and the
/// query mentions an `id` column — a proxy for a primary key making the
/// dedup unnecessary.
pub(super) fn sql_violates_union_all(sql: &str) -> bool {
    let upper = sql.to_ascii_uppercase();
    let Some(pos) = upper.find("UNION") else {
        return false;
    };
    let after = &upper[pos + "UNION".len()..];
    if after.trim_start().starts_with("ALL") {
        return false;
    }
    upper.contains("SELECT ID")
        || upper.contains(" ID,")
        || upper.contains(" ID ")
        || upper.contains(".ID")
}
