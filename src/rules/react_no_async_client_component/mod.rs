//! React client components must be synchronous. An `async` client component
//! throws at render time: "async/await is not yet supported in Client
//! Components." Only server components may be async.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-async-client-component",
    description: "Client components can't be `async` — only server components can.",
    remediation: "Make the component synchronous and fetch data via `useEffect` \
                  or an API route. To `await` during render, remove `\"use client\"` \
                  and run it as a server component.",
    severity: Severity::Error,
    doc_url: Some("https://react.dev/reference/rsc/use-client"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
