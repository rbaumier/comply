//! tailwind-prefer-shorthand

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-prefer-shorthand",
    description: "Collapse redundant Tailwind utility pairs into their shorthand form (e.g. `px-2 py-2` → `p-2`).",
    remediation: "Replace pairs like `pt-N pb-N` with `py-N`, `pl-N pr-N` with `px-N`, and `px-N py-N` with `p-N`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

pub fn register() -> RuleDef {
    let backends: Vec<_> = [
        Language::TypeScript,
        Language::Tsx,
        Language::JavaScript,
        Language::Vue,
    ]
    .into_iter()
    .map(|lang| (lang, Backend::Text(Box::new(text::Check))))
    .collect();
    RuleDef {
        meta: META,
        backends,
    }
}
