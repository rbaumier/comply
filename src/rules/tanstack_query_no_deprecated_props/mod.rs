//! tanstack-query-no-deprecated-props — v4 → v5 migration hints.

#[cfg(test)]
mod typescript;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

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
