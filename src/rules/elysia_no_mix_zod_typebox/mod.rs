//! elysia-no-mix-zod-typebox

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-no-mix-zod-typebox",
    description: "File mixes Zod and Elysia's TypeBox `t` for validation — pick one schema library.",
    remediation: "Standardize on Elysia's `t.Object(...)` for route validation. Zod schemas are not understood by Elysia's type inference.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["type-safety", "elysia"],

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
