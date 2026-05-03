mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-pass-signal-to-fetch",
    description: "A `queryFn` that destructures `{ signal }` should forward it to `fetch` for cancellation.",
    remediation: "Pass the signal through: `fetch(url, { signal })`. Otherwise in-flight requests won't be aborted when the query is cancelled.",
    severity: Severity::Warning,
    doc_url: Some("https://tanstack.com/query/v5/docs/react/guides/query-cancellation"),
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
