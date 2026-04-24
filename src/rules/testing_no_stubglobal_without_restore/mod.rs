//! testing-no-stubglobal-without-restore — flag `vi.stubGlobal()` /
//! `vi.stubEnv()` without a matching `unstubAllGlobals()` / `unstubAllEnvs()`
//! in the same file.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "testing-no-stubglobal-without-restore",
    description: "stubGlobal/stubEnv without unstubAllGlobals/unstubAllEnvs leaks mocked globals into sibling tests.",
    remediation: "Call vi.unstubAllGlobals() (or vi.unstubAllEnvs()) in afterEach/afterAll — or enable unstubGlobals/unstubEnvs in the Vitest config.",
    severity: Severity::Warning,
    doc_url: Some("https://vitest.dev/api/vi.html#vi-unstuballglobals"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
