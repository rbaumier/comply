//! api-validate-at-boundaries — flag `.parse(...)` / `.safeParse(...)`
//! calls in functions that don't look like request handlers or
//! middleware. Validation should happen once at the system boundary;
//! re-validating in internal helpers duplicates schemas and implies the
//! typed contract between internal functions isn't trusted.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "api-validate-at-boundaries",
    description: "Validation schemas (zod.parse) must run only at API boundaries, not between internal typed functions.",
    remediation: "Move the `.parse(...)` call to the HTTP handler or middleware. Internal callers should trust the static type contract.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api-design"],

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
