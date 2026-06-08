//! tanstack-query-no-query-in-render-loop — flag `useQuery` calls
//! inside `.map()` callbacks. Each row would create its own query
//! subscription, defeating dedup and bursting the network.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-no-query-in-render-loop",
    description: "`useQuery` inside `.map()` creates one subscription per row.",
    remediation: "Move the query out of the loop. Fetch the parent collection \
                  once, or use `useQueries` with a key per row.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["tanstack"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
