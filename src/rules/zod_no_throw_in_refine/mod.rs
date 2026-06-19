//! zod-no-throw-in-refine — throwing inside `.refine()` / `.superRefine()`
//! callbacks bypasses Zod's issue aggregation.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-no-throw-in-refine",
    description: "`throw` inside `.refine()` / `.superRefine()` bypasses Zod's issue aggregation and surfaces as an unhandled exception instead of a validation error.",
    remediation: "Use ctx.addIssue() in superRefine, or return false in refine",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],

    // In a test/spec file, a `throw` inside `.refine()` is the behavior under
    // test (e.g. asserting Zod re-throws / wraps it via `expect(...).toThrow()`),
    // not a production mistake — and such files never ship. Suppress there.
    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
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
