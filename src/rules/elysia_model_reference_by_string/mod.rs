//! elysia-model-reference-by-string

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-model-reference-by-string",
    description: "Routes that import a TypeBox schema variable and use it inline lose Elysia's model registry deduplication.",
    remediation: "Register the schema with `.model({ name: schema })` once and reference it as `body: 'name'`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["maintainability", "elysia"],

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
