//! no-mock-fetch-directly — use MSW instead of mocking HTTP clients.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-mock-fetch-directly",
    description: "Mocking `fetch`/`axios` directly couples tests to the HTTP client implementation.",
    remediation: "Use MSW (`msw`) to intercept at the network level instead \
                  of `vi.mock('axios')` or `global.fetch = vi.fn()`. MSW \
                  handlers are reusable, work with any HTTP client, and \
                  test real request/response cycles. Switching HTTP clients \
                  won't break your tests.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
