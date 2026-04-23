//! no-floating-promise — flag promise-returning calls whose result is
//! discarded at statement level.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-floating-promise",
    description: "Promise-returning call is used as a statement — rejection is ignored.",
    remediation: "`await` the promise, chain `.then/.catch`, pass it to \
                  `Promise.all`, or explicitly mark `void promise` if you \
                  intentionally ignore it. An unhandled rejection becomes an \
                  `UnhandledPromiseRejection` warning — and in Node 15+, crashes \
                  the process.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["async"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
