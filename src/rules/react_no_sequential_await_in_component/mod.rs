//! react-no-sequential-await-in-component — parallelise component data loads.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-sequential-await-in-component",
    description: "Sequential `await` of independent calls inside an async React \
                  component serialises fetches that could run in parallel.",
    remediation: "Wrap independent awaits in `Promise.all([...])`. Example: \
                  `const [user, posts] = await Promise.all([getUser(id), getPosts(id)])`. \
                  Server Components block rendering on each await, so chaining \
                  two fetches doubles the latency.",
    severity: Severity::Warning,
    doc_url: Some("https://react.dev/reference/rsc/server-components#async-components-with-server-components"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
