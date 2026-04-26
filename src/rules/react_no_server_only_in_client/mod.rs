//! `server-only` is a runtime poison pill — importing it from a client
//! component bundle throws at module evaluation. If the file is marked
//! `"use client"`, any `server-only` import is a contradiction.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-server-only-in-client",
    description: "`server-only` can't be imported from a client component.",
    remediation: "Remove `\"use client\"` from this file, or remove the \
                  `server-only` import and move server-side logic to a \
                  separate server module.",
    severity: Severity::Error,
    doc_url: Some("https://www.npmjs.com/package/server-only"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
