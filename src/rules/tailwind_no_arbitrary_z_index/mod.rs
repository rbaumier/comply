//! tailwind-no-arbitrary-z-index

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-arbitrary-z-index",
    description: "Arbitrary z-index values `z-[n]` bypass the design token scale.",
    remediation: "Use a design token (`z-10`, `z-50`) or define a custom token in `tailwind.config.ts`.",
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
