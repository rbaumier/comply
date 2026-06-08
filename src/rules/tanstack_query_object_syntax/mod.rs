mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-object-syntax",
    description: "TanStack Query v5 requires object syntax: `useQuery({ queryKey, queryFn })`.",
    remediation: "Replace the positional form `useQuery(key, fn, opts)` with `useQuery({ queryKey: key, queryFn: fn, ...opts })`.",
    severity: Severity::Error,
    doc_url: Some("https://tanstack.com/query/v5/docs/react/guides/migrating-to-v5"),
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
