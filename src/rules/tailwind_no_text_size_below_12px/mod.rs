//! tailwind-no-text-size-below-12px — flag arbitrary `text-[<10|11>px]`
//! values. Body text below 12px fails most accessibility audits.

#[cfg(test)]
mod typescript;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-text-size-below-12px",
    description: "Text below 12px fails accessibility audits and is hard to read.",
    remediation: "Use `text-xs` (12px) or larger.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind", "accessibility"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
