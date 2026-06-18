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

    skip_in_test_dir: false,
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
