mod typescript;
use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-max-pages-requires-both",
    description: "`maxPages` on an infinite query requires both `getNextPageParam` and `getPreviousPageParam`.",
    remediation: "Define both page-param functions, or remove `maxPages` — with only one, refetching the oldest pages is impossible.",
    severity: Severity::Error,
    doc_url: Some(
        "https://tanstack.com/query/v5/docs/react/guides/infinite-queries#what-if-i-want-to-limit-the-number-of-pages",
    ),
    categories: &["tanstack-query"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
