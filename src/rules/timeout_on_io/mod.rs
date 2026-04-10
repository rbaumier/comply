//! timeout-on-io — every I/O call needs a timeout.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "timeout-on-io",
    description: "I/O calls without a timeout can hang forever.",
    remediation: "Wrap the I/O call with `withTimeout(call, 5_000)` or pass \
                  `{ signal: AbortSignal.timeout(5_000) }`. Default \
                  timeouts: 5s for DB, 10s for external APIs, 30s for file ops.",
    severity: Severity::Warning,
    doc_url: None,
};pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
