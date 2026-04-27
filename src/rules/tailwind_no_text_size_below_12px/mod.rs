//! tailwind-no-text-size-below-12px — flag arbitrary `text-[<10|11>px]`
//! values. Body text below 12px fails most accessibility audits.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-text-size-below-12px",
    description: "Text below 12px fails accessibility audits and is hard to read.",
    remediation: "Use `text-xs` (12px) or larger.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind", "accessibility"],
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
