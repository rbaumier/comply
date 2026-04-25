mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-apply-for-variants",
    description: "`@apply` outside `@layer base` defeats Tailwind's purging and specificity model.",
    remediation: "Compose classes in JSX/HTML instead, or use CSS variables for theming. Reserve `@apply` for `@layer base` resets only.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Tsx, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Vue, Backend::TreeSitter(Box::new(typescript::Check))),
        ],
    }
}
