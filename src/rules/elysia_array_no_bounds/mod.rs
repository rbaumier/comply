//! elysia-array-no-bounds

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-array-no-bounds",
    description: "`t.Array(...)` is declared without `minItems` / `maxItems` — clients can DoS the API with huge payloads.",
    remediation: "Pass `{ minItems, maxItems }` as the second argument: `t.Array(t.String(), { maxItems: 100 })`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "elysia"],
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
