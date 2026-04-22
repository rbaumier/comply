//! testing-prefer-msw — flag direct HTTP-client mocking in tests.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "testing-prefer-msw",
    description: "Mocking HTTP clients directly is brittle — use MSW to intercept at the network layer.",
    remediation: "Replace `vi.mock('axios')` / `jest.mock('node-fetch')` / `global.fetch = vi.fn()` with an MSW request handler.",
    severity: Severity::Warning,
    doc_url: Some("https://mswjs.io/"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
