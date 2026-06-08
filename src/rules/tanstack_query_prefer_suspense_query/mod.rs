//! tanstack-query-prefer-suspense-query — prefer `useSuspenseQuery`
//! over `useQuery + if (isLoading|isPending) return …` in components
//! wrapped by a Suspense boundary.
//!
//! Why: `useSuspenseQuery` eliminates the manual loading-state boilerplate
//! and the `data: undefined` type narrowing every call site has to do.
//! The hook throws a promise that the nearest Suspense boundary catches,
//! and `data` is guaranteed defined on the happy path.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-prefer-suspense-query",
    description: "`useQuery` followed by `if (isLoading|isPending) return …` should use `useSuspenseQuery` instead.",
    remediation: "Replace `useQuery` with `useSuspenseQuery`, remove the \
                  early-return branch, and wrap the caller tree in a \
                  `<Suspense fallback={...}>` boundary. `data` will be \
                  guaranteed defined.",
    severity: Severity::Warning,
    doc_url: Some("https://tanstack.com/query/v5/docs/framework/react/reference/useSuspenseQuery"),
    categories: &["tanstack-query"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::TreeSitter(Box::new(typescript::Check))))
            .collect(),
    }
}
