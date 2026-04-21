//! tailwind-prefer-size-shorthand

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-prefer-size-shorthand",
    description: "`w-X h-X` with equal values can be written as `size-X`.",
    remediation: "Replace `w-4 h-4` with `size-4` (Tailwind v3.4+).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::Vue, Backend::Text(Box::new(text::Check))),
        ],
    }
}
