mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-no-global-onerror-v5",
    description: "`defaultOptions.queries.onError` was removed in v5 — use `QueryCache({ onError })`.",
    remediation: "Move the global error handler to the QueryCache: `new QueryClient({ queryCache: new QueryCache({ onError }) })`.",
    severity: Severity::Error,
    doc_url: Some(
        "https://tanstack.com/query/v5/docs/react/guides/migrating-to-v5#callbacks-on-usequery-have-been-removed",
    ),
    categories: &["tanstack-query"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
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
