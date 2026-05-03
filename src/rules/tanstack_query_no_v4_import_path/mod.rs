mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-no-v4-import-path",
    description: "The package `react-query` is the v3 / v4 name — v5 lives at `@tanstack/react-query`.",
    remediation: "Replace the import with `@tanstack/react-query` (and remove the old `react-query` dependency).",
    severity: Severity::Error,
    doc_url: Some("https://tanstack.com/query/v5/docs/react/guides/migrating-to-react-query-4"),
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
