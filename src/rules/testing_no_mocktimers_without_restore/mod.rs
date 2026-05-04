//! testing-no-mocktimers-without-restore — flag files that call
//! `vi.useFakeTimers()` / `jest.useFakeTimers()` but never pair it with
//! `useRealTimers()` in an afterEach/afterAll.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "testing-no-mocktimers-without-restore",
    description: "useFakeTimers() without a matching useRealTimers() in afterEach/afterAll leaks mocked timers into sibling tests.",
    remediation: "Call vi.useRealTimers() (or jest.useRealTimers()) in afterEach/afterAll to restore the real timers.",
    severity: Severity::Warning,
    doc_url: Some("https://vitest.dev/api/vi.html#vi-userealtimers"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
