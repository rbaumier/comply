//! tanstack-start-server-fn-use-notfound — prefer `throw notFound()` over
//! `throw new Error('not found')` in server functions.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-server-fn-use-notfound",
    description: "Server functions should throw `notFound()` rather than a generic Error.",
    remediation: "Replace `throw new Error('not found')` with `throw notFound()` \
                  so the router renders the proper 404 boundary.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-start"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
