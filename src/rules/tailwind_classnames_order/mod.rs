mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-classnames-order",
    description: "Tailwind classes should follow a canonical category order (layout → spacing → sizing → typography → visual).",
    remediation: "Reorder utility classes to follow the recommended group order. Tools like `prettier-plugin-tailwindcss` or `eslint-plugin-tailwindcss` can auto-fix this.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/tailwindlabs/prettier-plugin-tailwindcss"),
    categories: &["tailwind"],
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
                Language::Tsx,
                Backend::TreeSitter(Box::new(typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::TreeSitter(Box::new(typescript::Check)),
            ),
            (
                Language::Vue,
                Backend::TreeSitter(Box::new(typescript::Check)),
            ),
        ],
    }
}
