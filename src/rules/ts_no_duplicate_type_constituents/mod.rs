//! ts-no-duplicate-type-constituents — duplicates in unions / intersections.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-duplicate-type-constituents",
    description: "Duplicate members in a union or intersection are dead — TS resolves them to the same type but readers and refactors can be confused.",
    remediation: "Remove the duplicate type member. Use an alias if the repetition signals a missing concept.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-duplicate-type-constituents/"),
    categories: &["typescript"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
