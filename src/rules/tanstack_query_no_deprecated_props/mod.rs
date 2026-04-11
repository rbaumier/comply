//! tanstack-query-no-deprecated-props — v4 → v5 migration hints.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-no-deprecated-props",
    description: "Deprecated TanStack Query props from v4.",
    remediation: "Migrate to v5 names: `cacheTime` → `gcTime`, \
                  `useErrorBoundary` → `throwOnError`. `onSuccess`/`onError`/\
                  `onSettled` are removed from `useQuery` — use `useEffect` \
                  instead (mutation callbacks still work).",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "tanstack"],
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
