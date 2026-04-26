//! JSX event handlers (`onClick`, `onChange`, `onSubmit`, …) need the React
//! client runtime. Inside a server component they're inert at best and cause
//! hydration-time errors at worst. Flag them at authoring time.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-event-handler-in-server-component",
    description: "Event handlers (`onClick`, `onChange`, …) can't run in a server component.",
    remediation: "Move interactive JSX into a client component (`\"use client\"`), \
                  or use a server action via `<form action={...}>` for form submits.",
    severity: Severity::Error,
    doc_url: Some("https://react.dev/reference/rsc/server-components#serializable-props"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
