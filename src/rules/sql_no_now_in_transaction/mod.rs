//! sql-no-now-in-transaction

mod rust;
mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-now-in-transaction",
    description: "`NOW()` inside a transaction freezes at BEGIN time — use `clock_timestamp()` for real-time values.",
    remediation: "`NOW()`/`CURRENT_TIMESTAMP` return the transaction start time. For per-statement wall-clock, use `clock_timestamp()`.",
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
        ],
    }
}

/// Walk the SQL string line-by-line tracking transaction state and return true
/// if `NOW()` or `CURRENT_TIMESTAMP` appears inside a BEGIN..COMMIT block.
///
/// The input may include the AST node's surrounding quote characters (`` ` ``,
/// `"`, `r#"`, etc.) — we strip leading non-SQL punctuation per line before
/// matching keywords so the start/end detection works on the same text the
/// original TextCheck saw.
pub(super) fn sql_uses_now_in_tx(sql: &str) -> bool {
    let upper = sql.to_ascii_uppercase();
    let mut in_tx = false;
    for line in upper.lines() {
        // Strip both leading whitespace and surrounding string-literal punctuation
        // (`, ", #, r — for raw strings) so the BEGIN/COMMIT prefix checks work
        // on the SQL content even when the literal's opening quote sits on the
        // same line as the SQL.
        let trimmed = line
            .trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '`' | '"' | '#' | 'R'));
        if trimmed.starts_with("BEGIN;")
            || trimmed == "BEGIN"
            || trimmed.starts_with("BEGIN ")
            || trimmed.contains("START TRANSACTION")
        {
            in_tx = true;
            continue;
        }
        if trimmed.starts_with("COMMIT") || trimmed.starts_with("ROLLBACK") || trimmed == "END;" {
            in_tx = false;
            continue;
        }
        if !in_tx {
            continue;
        }
        if line.contains("NOW()") || line.contains("CURRENT_TIMESTAMP") {
            return true;
        }
    }
    false
}
