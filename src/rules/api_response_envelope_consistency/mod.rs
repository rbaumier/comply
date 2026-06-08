//! api-response-envelope-consistency — within a single API file, all
//! return shapes should agree: either every response is `{ data: ... }`
//! / `{ data, error }` or none are. Mixing creates a flaky client.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "api-response-envelope-consistency",
    description: "Mixing `{ data: ... }` envelopes with raw returns forces every client to branch.",
    remediation: "Pick one shape for the file (envelope or raw) and apply it to every response.",
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
            (
                Language::TypeScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
