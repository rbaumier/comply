mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-no-is-loading",
    description: "`isLoading` was renamed to `isPending` in TanStack Query v5.",
    remediation: "Replace `isLoading` with `isPending` (or `isFetching` if you need network activity).",
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
