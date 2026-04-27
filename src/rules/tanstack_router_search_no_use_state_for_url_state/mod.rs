//! tanstack-router-search-no-use-state-for-url-state — flag
//! `useState` for URL-state-shaped variables (`filter`, `page`, `sort`,
//! `tab`, `search`, `query`) in files that import `@tanstack/react-router`.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-router-search-no-use-state-for-url-state",
    description: "Filter / page / sort state belongs in the URL, not in `useState`.",
    remediation: "Use TanStack Router's `Route.useSearch()` and `navigate({ search })` so \
                  the state survives reloads and is shareable.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack"],
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
