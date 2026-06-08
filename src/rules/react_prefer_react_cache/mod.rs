//! react-prefer-react-cache — dedupe async data fetchers with `React.cache()`.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-prefer-react-cache",
    description: "Module-level async fetchers should be wrapped in `React.cache()` \
                  so multiple Server Components in the same render share one request.",
    remediation: "Wrap the async function in `React.cache(...)` (or `cache(...)` \
                  imported from `react`). Example: \
                  `export const getUser = cache(async (id) => { ... });`. Without \
                  `cache`, two Server Components that both call `getUser(1)` in the \
                  same render issue two separate network requests.",
    severity: Severity::Warning,
    doc_url: Some("https://react.dev/reference/react/cache"),
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
