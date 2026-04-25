mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-magic-spacing",
    description: "Arbitrary pixel spacing like `p-[13px]` breaks design-token consistency.",
    remediation: "Use the standard spacing scale (`p-1` = 4px, `p-2` = 8px, etc.) or arbitrary values that are multiples of 4.",
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
