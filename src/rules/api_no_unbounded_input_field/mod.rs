//! api-no-unbounded-input-field — flag zod fields used as request body
//! that lack a `.max(...)` constraint. An unbounded `z.string()` is a
//! resource-exhaustion vector. Response/output shapes and config/env
//! shapes (recognised by name suffix) are skipped — they are not HTTP
//! request inputs.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "api-no-unbounded-input-field",
    description: "API input fields without `.max(...)` are unbounded resource sinks.",
    remediation: "Add a `.max(N)` constraint to every `z.string()` / `z.number()` / `z.array()` in body schemas.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api-design", "security"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::TreeSitter(Box::new(typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::TreeSitter(Box::new(typescript::Check)),
            ),
            (
                Language::Tsx,
                Backend::TreeSitter(Box::new(typescript::Check)),
            ),
        ],
    }
}
