//! timeout-on-io — every I/O call needs a timeout.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "timeout-on-io",
    description: "I/O calls without a timeout can hang forever.",
    remediation: "Wrap the I/O call with `withTimeout(call, 5_000)` or pass \
                  `{ signal: AbortSignal.timeout(5_000) }`. Default \
                  timeouts: 5s for DB, 10s for external APIs, 30s for file ops.",
    severity: Severity::Warning,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::TreeSitter(Box::new(typescript::Check))))
            .collect(),
    }
}
