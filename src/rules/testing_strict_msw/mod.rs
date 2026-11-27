//! testing-strict-msw — flag MSW setups letting unhandled requests through.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "testing-strict-msw",
    description: "An MSW server or worker that warns on or bypasses unhandled requests lets tests reach the real network.",
    remediation: "Pass `{ onUnhandledRequest: \"error\" }` (msw 3: `onUnhandledFrame: \"error\"`) to `server.listen()` / `worker.start()`.",
    severity: Severity::Error,
    doc_url: Some("https://mswjs.io/docs/api/setup-server/listen"),
    categories: &["testing"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
