//! no-mock-fetch-directly — use MSW instead of mocking HTTP clients.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-mock-fetch-directly",
    description:
        "Mocking `fetch`/`axios` directly couples tests to the HTTP client implementation.",
    remediation: "Use MSW (`msw`) to intercept at the network level instead \
                  of `vi.mock('axios')` or `global.fetch = vi.fn()`. MSW \
                  handlers are reusable, work with any HTTP client, and \
                  test real request/response cycles. Switching HTTP clients \
                  won't break your tests.",
    severity: Severity::Warning,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
