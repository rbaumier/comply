//! no-catch-log-rethrow — flag `catch { log(e); throw e; }` patterns.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-catch-log-rethrow",
    description: "Catch that only logs and rethrows — the log duplicates the uncaught handler.",
    remediation: "Remove the catch block entirely. The error will propagate to the \
                  top-level handler which already logs it, so the local log just \
                  produces duplicate stack traces. Catch only when you add value: \
                  wrap with context, recover, translate.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["error-handling"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
