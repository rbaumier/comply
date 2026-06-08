//! elysia-response-t-unknown

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-response-t-unknown",
    description: "`response: t.Unknown()` / `t.Any()` disables response validation, so Eden inherits no type-safety.",
    remediation: "Describe the response with a concrete TypeBox schema (`t.Object({...})`, `t.String()`, etc.).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],

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
