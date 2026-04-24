//! react-no-use-client-without-client-api — `"use client"` in a file with no client-only APIs.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-use-client-without-client-api",
    description: "`\"use client\"` directive in a file that uses no hooks, event handlers, or browser APIs.",
    remediation: "Remove the `\"use client\"` directive so the module can render on the server, \
                  or add the client-only behavior that justifies it.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
