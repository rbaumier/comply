//! tanstack-query-dehydrate-no-pending-in-ssr — flag `dehydrate(...)`
//! preceded by an unawaited `prefetchQuery`. Pending queries serialize
//! as empty, so the client refetches everything anyway.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-dehydrate-no-pending-in-ssr",
    description: "Calling `dehydrate(...)` while `prefetchQuery` is still pending serializes empty state.",
    remediation: "`await` every `prefetchQuery(...)` before `dehydrate(queryClient)`.",
    severity: Severity::Warning,
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
