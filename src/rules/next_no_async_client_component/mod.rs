//! next-no-async-client-component

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-async-client-component",
    description: "Client components cannot be `async` — they must be synchronous.",
    remediation: "Drop `async`, fetch via `useEffect`/`useSWR`/`useQuery`, or convert this file to a server component.",
    severity: Severity::Error,
    doc_url: Some("https://react.dev/reference/rsc/use-client"),
    categories: &["nextjs", "rsc"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
