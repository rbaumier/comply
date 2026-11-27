//! testing-no-shared-signal: abort signals shared across runs.
//! Test files: module or describe timeouts are reported.
//! Controllers count when two tests read them.
//! Elsewhere: module timeouts read per request are reported.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "testing-no-shared-signal",
    description: "An AbortSignal timeout or AbortController created once is shared by several tests or requests — later ones inherit a spent or aborted signal.",
    remediation: "Create the signal where the request is made, and the controller inside the test or in beforeEach().",
    severity: Severity::Error,
    doc_url: None,
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
