//! tanstack-query-no-query-in-effect — `useQuery()` already has its own
//! lifecycle; calling it from a `useEffect` defeats the point.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-no-query-in-effect",
    description: "TanStack Query hook called inside `useEffect`.",
    remediation: "Call `useQuery` at the top level of the component — it \
                  manages its own subscriptions, refetching, and cleanup.",
    severity: Severity::Warning,
    doc_url: None,
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
