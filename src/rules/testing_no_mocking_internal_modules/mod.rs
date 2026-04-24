//! testing-no-mocking-internal-modules — flag `vi.mock('./...')` / `jest.mock('./...')`
//! of relative internal paths. Tests should mock boundaries, not internals.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "testing-no-mocking-internal-modules",
    description: "Mocking a relative internal module couples tests to implementation details.",
    remediation: "Mock only external boundaries (HTTP, DB, third-party SDKs). Refactor so the collaborator is injected, or rely on the real internal module.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
