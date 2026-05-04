mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-no-enabled-on-suspense",
    description: "`useSuspenseQuery` does not support `enabled`.",
    remediation: "Conditionally render the component that calls `useSuspenseQuery` instead, or fall back to `useQuery` when you need to gate the request.",
    severity: Severity::Error,
    doc_url: Some("https://tanstack.com/query/v5/docs/react/reference/useSuspenseQuery"),
    categories: &["tanstack-query"],
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
