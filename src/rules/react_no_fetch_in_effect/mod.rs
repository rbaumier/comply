//! react-no-fetch-in-effect — `fetch()` inside `useEffect` is fragile (no
//! deduping, caching, retries, race protection). Prefer a data-fetching
//! library or a server component.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-fetch-in-effect",
    description: "`fetch()` inside `useEffect` lacks caching, deduping, and race protection.",
    remediation: "Use a data-fetching library (TanStack Query, SWR) or move \
                  fetching to a server component / loader.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],

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
