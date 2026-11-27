//! testing-no-sleep-in-test — fixed sleeps run by JS/TS tests.
//!
//! Exempt: handed-off callbacks, zero delays, poll loops, fake timers.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "testing-no-sleep-in-test",
    description: "No fixed-delay sleep inside JS/TS tests.",
    remediation: "A fixed wait makes the test slow and flaky. Wait on the \
                  condition itself (`waitFor`, `expect.poll`, an event or a \
                  promise), or drive time with fake timers \
                  (`vi.useFakeTimers()` + `vi.advanceTimersByTime(ms)`).",
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
