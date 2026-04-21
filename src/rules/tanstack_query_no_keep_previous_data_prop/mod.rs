mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-no-keep-previous-data-prop",
    description: "`keepPreviousData: true` was replaced by `placeholderData: keepPreviousData` in v5.",
    remediation: "Import `keepPreviousData` from `@tanstack/react-query` and use `placeholderData: keepPreviousData`.",
    severity: Severity::Warning,
    doc_url: Some("https://tanstack.com/query/v5/docs/react/guides/migrating-to-v5"),
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
