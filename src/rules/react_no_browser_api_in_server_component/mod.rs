//! Browser globals (`window`, `document`, `localStorage`, …) are undefined
//! during server render. Touching them in a server component crashes the
//! request at render time — flag early.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-browser-api-in-server-component",
    description: "Browser globals (`window`, `document`, `localStorage`) don't exist on the server.",
    remediation: "Move the browser-only code into a `\"use client\"` component, \
                  gate it behind `useEffect`, or use a server-safe alternative.",
    severity: Severity::Error,
    doc_url: Some("https://react.dev/reference/rsc/server-components"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
