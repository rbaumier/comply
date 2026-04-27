//! api-no-unbounded-input-field — flag zod fields used as request body
//! that lack a `.max(...)` constraint. An unbounded `z.string()` is a
//! resource-exhaustion vector.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "api-no-unbounded-input-field",
    description: "API input fields without `.max(...)` are unbounded resource sinks.",
    remediation: "Add a `.max(N)` constraint to every `z.string()` / `z.number()` / `z.array()` in body schemas.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api-design", "security"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::Text(Box::new(typescript::Check))),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
