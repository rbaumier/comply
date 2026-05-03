//! testing-no-stubglobal-without-restore — flag `vi.stubGlobal()` /
//! `vi.stubEnv()` without a matching `unstubAllGlobals()` / `unstubAllEnvs()`
//! in the same file.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "testing-no-stubglobal-without-restore",
    description: "stubGlobal/stubEnv without unstubAllGlobals/unstubAllEnvs leaks mocked globals into sibling tests.",
    remediation: "Call vi.unstubAllGlobals() (or vi.unstubAllEnvs()) in afterEach/afterAll — or enable unstubGlobals/unstubEnvs in the Vitest config.",
    severity: Severity::Warning,
    doc_url: Some("https://vitest.dev/api/vi.html#vi-unstuballglobals"),
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
