//! vitest-no-done-callback — legacy Jest-style `done` callback.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vitest-no-done-callback",
    description: "Vitest does not support the legacy Jest `done` callback — the test will silently never finish.",
    remediation: "Return a Promise or mark the callback `async` and `await` the assertions. Drop the `done` parameter.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["testing", "vitest"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
