//! react-prefer-react-cache — dedupe async data fetchers with `React.cache()`.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

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
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
