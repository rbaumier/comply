//! vitest-no-focused-tests — flag `.only` on test / it / describe.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vitest-no-focused-tests",
    description: "`.only` on `test`, `it`, or `describe` skips the rest of the suite — usually a left-over from local debugging.",
    remediation: "Remove `.only` before committing. If you really want to isolate a test, use the runner's `--testNamePattern` flag instead.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["testing", "vitest", "playwright"],

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
