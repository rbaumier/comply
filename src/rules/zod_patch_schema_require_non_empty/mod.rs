//! zod-patch-schema-require-non-empty — an all-optional Zod PATCH/Edit/Update
//! schema accepts an empty `{}` body, producing a silent no-op update.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-patch-schema-require-non-empty",
    description: "An all-optional Zod schema for a PATCH/Edit/Update body accepts \
                  an empty `{}`, validating a request that updates nothing.",
    remediation: "Add a `.refine(o => Object.keys(o).length >= 1, { error: \"At least \
                  one field required\" })` guard so the schema rejects an empty body, \
                  or make at least one field required.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],

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
