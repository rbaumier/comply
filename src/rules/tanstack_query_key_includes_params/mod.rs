//! tanstack-query-key-includes-params — queryKey must include every
//! non-parameter identifier referenced by queryFn.
//!
//! Why: TanStack Query treats the queryKey as the cache identity. If
//! queryFn closes over a variable (`userId`, `filter`, …) that is not
//! in the queryKey, two different logical queries will collide on the
//! same cache slot — the first result is shown for both and no refetch
//! happens when the closure variable changes.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-key-includes-params",
    description: "`queryKey` must include every non-parameter identifier referenced by `queryFn`.",
    remediation: "Add the missing identifier(s) to the `queryKey` array so the cache is \
                  keyed on every dynamic input. Example: `useQuery({ queryKey: ['user', userId], \
                  queryFn: () => fetchUser(userId) })`.",
    severity: Severity::Error,
    doc_url: None,
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
