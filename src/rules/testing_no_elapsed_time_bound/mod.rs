//! testing-no-elapsed-time-bound — upper bound on a `Date.now()`/`performance.now()` delta.
//!
//! The delta may come through a same-file `const`.
//! Exempt: lower bounds, benchmarks, non-test files.
//! Exempt: scopes or hooks calling `useFakeTimers()` directly.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "testing-no-elapsed-time-bound",
    description: "No upper bound on measured wall-clock time inside JS/TS tests.",
    remediation: "A wall-clock budget fails on a loaded CI machine. Assert the \
                  order of events or count the steps taken, drive time with \
                  fake timers, or keep a margin wide enough to catch only a \
                  pathological slowdown.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["testing"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Oxc(Box::new(oxc_typescript::Check))))
            .collect(),
    }
}
