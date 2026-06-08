//! tanstack-query-no-rest-destructuring — `const { data, ...rest } = useQuery()`
//! subscribes to every field on the query result and re-renders on every state
//! transition.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-no-rest-destructuring",
    description: "Rest destructuring on a TanStack Query result subscribes to every field.",
    remediation: "Destructure only the fields you actually need (e.g. `data`, \
                  `isLoading`) instead of using `...rest`.",
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
