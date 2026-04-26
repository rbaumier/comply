//! react-async-server-action — server actions must be async.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-async-server-action",
    description: "Server actions (functions with `\"use server\"`) must be `async`.",
    remediation: "Add `async` to the function. React Server Actions must be async \
                  functions — a synchronous function with `\"use server\"` will \
                  cause a build error or runtime failure.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
