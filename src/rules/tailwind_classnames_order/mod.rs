//! tailwind-classnames-order

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-classnames-order",
    description: "Tailwind classes should follow a canonical category order (layout → spacing → sizing → typography → visual).",
    remediation: "Reorder utility classes to follow the recommended group order. Tools like `prettier-plugin-tailwindcss` or `eslint-plugin-tailwindcss` can auto-fix this.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/tailwindlabs/prettier-plugin-tailwindcss"),
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
