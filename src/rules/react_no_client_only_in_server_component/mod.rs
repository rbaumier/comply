//! `client-only` is the mirror of `server-only`: importing it from a server
//! component throws at module evaluation. Flag the mismatch early.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-client-only-in-server-component",
    description: "`client-only` can't be imported from a server component.",
    remediation: "Mark the file `\"use client\"`, or remove the `client-only` \
                  import and keep the module server-safe.",
    severity: Severity::Error,
    doc_url: Some("https://www.npmjs.com/package/client-only"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
