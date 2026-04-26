//! React hooks can't run inside a server component — they need a client
//! runtime. Flagging early means developers see the violation in their editor
//! instead of at render time.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-client-hook-in-server-component",
    description: "React hooks can only run in client components.",
    remediation: "Add `\"use client\"` at the top of the file, or move the hook \
                  call into a separate client component and import it.",
    severity: Severity::Error,
    doc_url: Some("https://react.dev/reference/rsc/server-components#serializable-props"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
