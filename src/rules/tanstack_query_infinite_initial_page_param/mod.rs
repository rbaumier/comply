mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-infinite-initial-page-param",
    description: "`useInfiniteQuery` and `infiniteQueryOptions` require `initialPageParam` in v5.",
    remediation: "Add `initialPageParam` to the options object: `useInfiniteQuery({ queryKey, queryFn, initialPageParam: 0, getNextPageParam })`.",
    severity: Severity::Error,
    doc_url: Some(
        "https://tanstack.com/query/v5/docs/react/guides/migrating-to-v5#infinite-queries",
    ),
    categories: &["tanstack-query"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
