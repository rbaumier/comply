mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-prefer-key-factory",
    description: "Inline dynamic `queryKey` arrays should use a key factory for consistency.",
    remediation: "Define a key factory: `const todoKeys = { detail: (id: string) => ['todos', id] as const }` and use `todoKeys.detail(id)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack"],
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
