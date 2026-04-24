//! testing-no-mocktimers-without-restore — flag files that call
//! `vi.useFakeTimers()` / `jest.useFakeTimers()` but never pair it with
//! `useRealTimers()` in an afterEach/afterAll.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "testing-no-mocktimers-without-restore",
    description: "useFakeTimers() without a matching useRealTimers() in afterEach/afterAll leaks mocked timers into sibling tests.",
    remediation: "Call vi.useRealTimers() (or jest.useRealTimers()) in afterEach/afterAll to restore the real timers.",
    severity: Severity::Warning,
    doc_url: Some("https://vitest.dev/api/vi.html#vi-userealtimers"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
