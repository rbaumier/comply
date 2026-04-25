//! sql-require-transaction-timeout

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "sql-require-transaction-timeout",
    description: "DB connection pool config should set `statement_timeout` and `idle_in_transaction_session_timeout` to prevent runaway queries.",
    remediation: "Add `statement_timeout: '30s'` and `idle_in_transaction_session_timeout: '60s'` to the pool config.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
