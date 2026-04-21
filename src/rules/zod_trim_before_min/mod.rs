//! zod-trim-before-min — `.min(1)` without `.trim()` accepts whitespace-only strings.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "zod-trim-before-min",
    description: "`z.string().min(1)` without `.trim()` allows strings of only whitespace.",
    remediation: "Add `.trim()` before `.min(1)`: `z.string().trim().min(1)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
        ],
    }
}
